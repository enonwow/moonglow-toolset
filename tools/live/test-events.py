"""Private NWNX event proof; run via test-events.sh in WSL. No Python packages needed."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import secrets
import re
import select
import shutil
import socket
import struct
import subprocess
import time
import zipfile
import fcntl

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "target/test-output/live-events"
SCRATCH = Path.home() / "mglive-events"
OWNER = "moonglow-live-events-v1"
PREFIX = "mglive-events"
RPORT, GPORT = 6392, 5126
PLUGINS = Path(os.environ.get("NWNX_ROOT", "~/nwnx-redis-reconnect")).expanduser() / "Binaries"
SERVER = Path(os.environ.get("MGLIVE_NWSERVER", "~/server/bin/linux-x86/nwserver-linux")).expanduser()
# The Windows PowerShell 7 used by the Live helpers: MGLIVE_PWSH (tools/live/local.ps1) or the PATH.
PWSH = Path(os.environ.get("MGLIVE_PWSH") or shutil.which("pwsh.exe") or shutil.which("pwsh") or "pwsh")
GAME = "C:/Program Files (x86)/Steam/steamapps/common/Neverwinter Nights"


def dump(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def polish_tlk_bytes(index=0):
    data=(OUT / "mget_polish.tlk").read_bytes()
    start=struct.unpack_from("<I",data,16)[0]
    offset,length=struct.unpack_from("<II",data,20+40*index+28)
    return data[start+offset:start+offset+length]


def win(path):
    return subprocess.check_output(["wslpath", "-w", str(path)], text=True).strip()


def compile_fixture():
    """Use the compiler library through guarded tests, never an app binary/build."""
    existing_test = os.environ.get("MGLIVE_FIXTURE_TEST")
    if existing_test:
        # The same guarded fixture without requiring a PowerShell 7 installation
        # or competing with another agent's Cargo build. No server in this test.
        binary = Path(existing_test).resolve()
        assert binary.parent == (REPO / "target/debug/deps").resolve()
        assert re.fullmatch(r"private_fixture-[a-f0-9]+\.exe", binary.name) and binary.is_file()
        env = dict(os.environ, MGLIVE_PREPARE_EVENTS="1", MGLIVE_RESOURCE_ROOT=GAME,
                   NWN_ROOT="Z:/moonglow-no-native-game", CARGO_INCREMENTAL="0")
        env["WSLENV"] = env.get("WSLENV", "") + ":MGLIVE_PREPARE_EVENTS:MGLIVE_RESOURCE_ROOT:MGLIVE_STRESS_PREPARE:NWN_ROOT:CARGO_INCREMENTAL"
        subprocess.run([str(binary), "--exact", "prepare_private_events_fixture", "--nocapture"], env=env, check=True)
        result = json.loads((OUT / "compile.json").read_text())
        result["route"] = "existing guarded private_fixture test; one compiler worker"
        dump(OUT / "compile.json", result)
        return result
    existing = os.environ.get("MGLIVE_FIXTURE_MG")
    if existing:
        # Explicit fallback while another agent owns Cargo's build lock. Use an
        # already-built compiler, never build/launch a GUI or touch demo output.
        binary = Path(existing).resolve()
        assert binary in [(REPO / f"target/{profile}/mg.exe").resolve() for profile in ("debug", "release")] and binary.is_file()
        module = OUT / "module"
        assert module.resolve().parent == OUT.resolve() and not module.is_symlink()
        def mg(*args):
            command=[str(binary),"--root",GAME,"--no-user-dir","--json",*map(str,args)]
            completed=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            if completed.returncode: raise RuntimeError(completed.stdout.decode(errors="replace") + completed.stderr.decode(errors="replace"))
            return completed.stdout
        for name in ["module.ifo","events.are","events.git","events.gic","events2.are","events2.git","events2.gic","repute.fac","mget_chest.utp","mget_door.utd","mget_polish.dlg","mget_focus.dlg","mget_polish.utp","mget_waypoint.utw","mget_encounter.ute","mget_store.utm","mget_trigger.utt"]:
            source=OUT/(name+".json")
            if name=="mget_polish.utp":
                # The CLI's GFF writer uses cp1252; express the exact authored
                # cp1250 bytes losslessly in that writer's input alphabet.
                value=json.loads(source.read_text())
                for entry in [value["Tag"],value["LocName"]["value"]]:
                    key="value" if "value" in entry else "0"
                    entry[key]=entry[key].encode("cp1250").decode("cp1252")
                source=OUT/"mget_polish-cli.utp.json";dump(source,value)
            mg("gff",win(source),"-o",win(module/name))
            if name=="mget_polish.utp": source.unlink()
        if os.environ.get("MGLIVE_CAPTURE_PROOF")=="1":
            raise RuntimeError("Dialogue capture proof requires the guarded Rust fixture compiler")
        compiled=json.loads(mg("compile",win(module)))
        # Older CLI builds try standalone includes too. Their missing context
        # is expected; every executable entry point must still compile.
        errors = compiled.get("errors", [])
        includes = [e for e in errors if not re.search(
            r"\b(?:void\s+main|int\s+StartingConditional)\s*\(",
            (module / (e["script"] + ".nss")).read_text())]
        assert compiled.get("failed", 0) == len(includes), compiled
        compiled["include_diagnostics"] = includes
        compiled["failed"] = 0
        mg("pack",win(module),win(OUT/"events.mod"))
        result={"failed":0,"compiler_binary":win(binary),"route":"existing mg compiler; no app build", "native_compile":compiled}
        dump(OUT/"compile.json",result)
        return result
    env = dict(os.environ, MGLIVE_PREPARE_EVENTS="1", MGLIVE_RESOURCE_ROOT=GAME)
    env["WSLENV"] = env.get("WSLENV", "") + ":MGLIVE_PREPARE_EVENTS:MGLIVE_RESOURCE_ROOT:MGLIVE_STRESS_PREPARE"
    subprocess.run([str(PWSH), "-NoProfile", "-File", win(REPO / "tools/live/check.ps1"),
                    "-Phase", "5", "-Step", "events-prepare", "-Package", "mg-live", "-Filter", "prepare_private_events_fixture",
                    "-Jobs", "1", "-NoUiDebug", "-UiUnoptimized"], env=env, check=True)
    return json.loads((OUT / "compile.json").read_text())


def field(kind, value):
    return {"type": kind, "value": value}


def fixture():
    """Small authored module, no copied demo, custom HAKs or extracted asset tree."""
    module = OUT / "module"
    module.mkdir(exist_ok=True)
    # Retire only this owned fixture's removed scripts, never a user module.
    for stem in ['mglive_animation_replace','mglive_colour','mglive_material','mglive_shader','mget_shader']:
        for suffix in ['nss','ncs','ndb']:
            retired=module/(stem+'.'+suffix)
            if retired.exists():
                assert (OUT/'.mglive-events-fixture').read_text().strip()==OWNER
                retired.unlink()
    f = field
    for kind, ext in [("waypoint","utw"),("encounter","ute"),("store","utm"),("trigger","utt")]:
        r="mget_"+kind
        value={"__data_type":ext.upper()+" ","Tag":f("cexostring",r),"TemplateResRef":f("resref",r),"LocName":f("cexolocstring",{"0":"Audit "+kind})}
        dump(OUT/(r+"."+ext+".json"),value)
    info = {"__data_type": "IFO ", "Mod_Name": f("cexolocstring", {"0": "Live events proof"}),
            "Mod_Tag": f("cexostring", "MGLIVE_EVENTS"), "Mod_MinGameVer": f("cexostring", "1.89"),
            "Mod_CustomTlk": f("cexostring", "mget_polish"),
            "Mod_Version": f("dword", 3), "Expansion_Pack": f("word", 3),
            "Mod_IsSaveGame": f("byte", 0), "Mod_OnModLoad": f("resref", "mglive_load"),
            "Mod_Entry_Area": f("resref", "events"), "Mod_Entry_X": f("float", 10),
            "Mod_Entry_Y": f("float", 10), "Mod_Entry_Z": f("float", 0),
            "Mod_Entry_Dir_X": f("float", 0), "Mod_Entry_Dir_Y": f("float", 1),
            "Mod_Area_list": f("list", [{"__struct_id": 6, "Area_Name": f("resref", name)} for name in ["events", "events2"]]),
            "Mod_HakList": f("list", []), "Mod_StartYear": f("dword", 1372),
            "Mod_StartMonth": f("byte", 6), "Mod_StartDay": f("byte", 1),
            "Mod_StartHour": f("byte", 12), "Mod_MinPerHour": f("byte", 2)}
    tile = {"__struct_id": 1, **{key: f("int", 0) for key in
            ["Tile_ID", "Tile_Orientation", "Tile_Height"]}}
    # ttr01.set TILE50: ttr01_g01_01, four Grass corners at height 0.
    # TILE0 is a slope: it stranded the two actors at different Z coordinates.
    tile["Tile_ID"] = f("int", 50)
    tile.update({key: f("byte", 0) for key in ["Tile_MainLight1", "Tile_MainLight2",
                 "Tile_SrcLight1", "Tile_SrcLight2", "Tile_AnimLoop1", "Tile_AnimLoop2", "Tile_AnimLoop3"]})
    area = {"__data_type": "ARE ", "Name": f("cexolocstring", {"0": "Events"}),
            "Tag": f("cexostring", "EVENTS"), "ResRef": f("resref", "events"),
            "Tileset": f("resref", "ttr01"), "Width": f("int", 2), "Height": f("int", 2),
            "Tile_List": f("list", [tile] * 4), "ID": f("int", -1), "Version": f("dword", 1),
            "Flags": f("dword", 0), "PlayerVsPlayer": f("byte", 2)}
    lists = ["Creature List", "Door List", "Encounter List", "List", "SoundList", "StoreList",
             "TriggerList", "WaypointList", "Placeable List"]
    git = {"__data_type": "GIT ", **{k: f("list", []) for k in lists}}
    # Doors cannot be made by native CreateObject: they must be placed in GIT.
    git["Door List"]=f("list",[{"__struct_id":8,"TemplateResRef":f("resref","mget_door"),
        "Tag":f("cexostring","mget_door"),"LocName":f("cexolocstring",{"0":"Proof door"}),
        "GenericType":f("byte",1),"Appearance":f("dword",1),"Useable":f("byte",1),
        "HP":f("short",50),"CurrentHP":f("short",50),"Faction":f("dword",2),
        "X":f("float",10),"Y":f("float",10),"Z":f("float",0),"Bearing":f("float",0)}])
    gic = {"__data_type": "GIC ", **{k: f("list", []) for k in lists}}
    faction_names = ["PC", "Hostile", "Commoner", "Merchant", "Defender", "Agents", "Session 1", "Session 2", "test"]
    fac = {"__data_type": "FAC ", "FactionList": f("list", [
        {"__struct_id": i, "FactionName": f("cexostring", name),
         "FactionGlobal": f("word", 1), "FactionParentID": f("dword", 4294967295)}
        for i, name in enumerate(faction_names)]), "RepList": f("list", [
        {"__struct_id": a * (len(faction_names)-1) + b - 1, "FactionID1": f("dword", a),
         "FactionID2": f("dword", b), "FactionRep": f("dword", 100 if a == b else 0 if 1 in (a, b) else 50)}
        for a in range(len(faction_names)) for b in range(1, len(faction_names))])}
    for name, data in [("module.ifo", info), ("events.are", area), ("events.git", git),
                       ("events.gic", gic), ("repute.fac", fac)]:
        source = OUT / (name + ".json")
        dump(source, data)

    # Second tiny authored area for cross-area teleport proof, same working set.
    area2 = dict(area, Name=f("cexolocstring", {"0":"Destination"}), Tag=f("cexostring","EVENTS2"), ResRef=f("resref","events2"))
    for name,data in [("events2.are",area2),("events2.git",{"__data_type":"GIT ",**{k:f("list",[]) for k in lists}}),("events2.gic",gic)]:
        dump(OUT/(name+".json"),data)

    # Authored non-static chest/door fixtures, using only base-game appearances.
    for name, kind, appearance in [("mget_chest.utp", "UTP ", 4), ("mget_door.utd", "UTD ", 1)]:
        data={"__data_type":kind,"TemplateResRef":f("resref",name.split(".")[0]),
              "Tag":f("cexostring",name.split(".")[0]),"LocName":f("cexolocstring",{"0":name}),
              "Appearance":f("dword",appearance),"GenericType":f("byte",appearance),
              "Static":f("byte",0),"Useable":f("byte",1),"HasInventory":f("byte",1),
              "HP":f("short",50),"CurrentHP":f("short",50),"Faction":f("dword",2)}
        source=OUT/(name+".json")
        dump(source,data)

    subprocess.run(["python3", str(REPO / "tools/live/wsl/extract-includes.py"),
                    str(PLUGINS / "NWScript.zip"), str(module)], check=True, stdout=subprocess.DEVNULL)
    for path in (REPO / "tools/live/nwn").glob("*.nss"):
        shutil.copyfile(path, module / path.name)
    if os.environ.get("MGLIVE_LATENCY_BASELINE") == "1":
        old = subprocess.check_output(["git", "show", "0664e4c:tools/live/nwn/mglive_chat.nss"], cwd=REPO)
        (module / "mglive_chat.nss").write_bytes(old)
    if os.environ.get("MGLIVE_PATROL_BASELINE") == "1":
        # Immutable old implementation in the private fixture only. Never
        # checkout/replace the shared working-tree script for a negative control.
        old = subprocess.check_output(["git", "show", "c51cec4:tools/live/nwn/mglive_patrol.nss"], cwd=REPO)
        (module / "mglive_patrol.nss").write_bytes(old)
    for path in (REPO / "tools/live/fixtures/events").glob("*.nss"):
        shutil.copyfile(path, module / path.name)
    # An authored module conversation uses this custom TLK line. Its bytes are
    # genuinely cp1250; no command/publisher round trip supplies this speech.
    phrase = "Naprawd\u0119: \u0105\u0119\u0142\u0144\u00f3\u015b\u017c\u017a"
    raw = phrase.encode("cp1250")
    lines=[raw, "Naprawdę sama widziałam".encode("cp1250"), "Teraz już każdy o tym wie".encode("cp1250")]
    lines.append("Odpowied\u017a: \u0105\u0119\u0142\u0144\u00f3\u015b\u017c\u017a".encode("cp1250"))
    offset=0; entries=[]
    for line in lines:
        entries.append(struct.pack("<I16sIIII f", 1, b"", 0, 0, offset, len(line), 0)); offset+=len(line)
    (OUT / "mget_polish.tlk").write_bytes(struct.pack("<8sIII", b"TLK V3.0", 5, len(lines), 20+40*len(lines)) + b"".join(entries)+b"".join(lines))
    dump(OUT / "mget_polish.dlg.json", {"__data_type":"DLG ",
        "EntryList":f("list",[{"__struct_id":0,"Text":{"type":"cexolocstring","id":16777216,"value":{}},"RepliesList":f("list",[])}]),
        "ReplyList":f("list",[]),"StartingList":f("list",[{"__struct_id":0,"Index":f("dword",0),"Active":f("resref","")}])})
    def link(index):
        return {"__struct_id":0,"Index":f("dword",index),"Active":f("resref","mget_dlgcond"),"IsChild":f("byte",0)}
    def node(index,reply=False):
        return {"__struct_id":index,"Text":{"type":"cexolocstring","id":16777219 if reply else 16777216,"value":{}},
                "Script":f("resref","mget_dlgline"),"Delay":f("dword",1),"Speaker":f("cexostring",""),
                "ActionParams":f("list",[{"__struct_id":0,"Key":f("cexostring","preserved"),"Value":f("cexostring","original-param")}]),
                "EntriesList" if reply else "RepliesList":f("list",[] if reply else [link(0)])}
    dump(OUT/"mget_focus.dlg.json",{"__data_type":"DLG ","EntryList":f("list",[node(0)]),
        "ReplyList":f("list",[node(0,True)]),"StartingList":f("list",[link(0)]),
        "DelayEntry":f("dword",1),"DelayReply":f("dword",1),"PreventZoomIn":f("byte",1)})
    dump(OUT / "mget_polish.utp.json", {"__data_type":"UTP ","TemplateResRef":f("resref","mget_polish"),
        "Tag":f("cexostring",phrase),"LocName":f("cexolocstring",{"0":phrase}),
        "Appearance":f("dword",4),"Static":f("byte",0),"Useable":f("byte",1),"HP":f("short",10)})
    (OUT / ".mglive-events-fixture").write_text(OWNER+"\n")
    if os.environ.get("MGLIVE_STRESS_PREPARE") == "1":
        from stress_native import prepare
        prepare(OUT, module)
    if os.environ.get("MGLIVE_DENSE_PROOF") == "1":
        from test_dense_report import prepare
        prepare(module)
    if os.environ.get("MGLIVE_XL_PROOF") == "1":
        from test_chat_translation import prepare
        prepare(module)
    result = compile_fixture()
    assert result["failed"] == 0, result
    for script in ["mglive_load", "mglive_tick", "mglive_chat", "mglive_ps"]:
        assert (module / (script + ".ncs")).is_file(), script
    return OUT / "events.mod"


class Redis:
    def __init__(self):
        self.sock = socket.create_connection(("127.0.0.1", RPORT), 3)
        self.sock.settimeout(None)
        self.file = self.sock.makefile("rb", buffering=0)

    def send(self, *args):
        args = [str(a).encode() for a in args]
        self.sock.sendall(b"*%d\r\n" % len(args) + b"".join(
            b"$%d\r\n" % len(a) + a + b"\r\n" for a in args))

    def read(self):
        line = self.file.readline()
        if not line:
            raise EOFError("Redis closed")
        kind, data = line[:1], line[1:-2]
        if kind == b"*":
            return [self.read() for _ in range(int(data))]
        if kind == b"$":
            n = int(data)
            if n < 0:
                return None
            chunks = bytearray()
            while len(chunks) < n + 2:
                chunk = self.file.read(n + 2 - len(chunks))
                if not chunk:
                    raise EOFError("Redis partial response")
                chunks.extend(chunk)
            return bytes(chunks[:-2]).decode()
        if kind == b"-":
            raise RuntimeError(data.decode())
        return int(data) if kind == b":" else data.decode()

    def close(self):
        self.file.close()
        self.sock.close()


class Proof:
    def chat_latency(self):
        return self.rust_api("chat_latency_before" if os.environ.get("MGLIVE_LATENCY_BASELINE")=="1" else "chat_latency")

    def __init__(self):
        self.pub, self.sub = Redis(), Redis()
        self.sub.send("SUBSCRIBE", PREFIX + ".state", PREFIX + ".log")
        self.sub.read()
        self.sub.read()
        self.frames, self.logs, self.events = [], [], {}
        self.raw_sizes = {}
        self.counter = 0
        self.state = None
        self.stream_cursor = "0-0"

    def poll(self, timeout=1):
        from stream_transport import read_events
        stream_rows = []
        # Drain bounded pages of retained older boots before waiting on state;
        # otherwise a reused 10k-row queue consumes the whole readiness timeout.
        for _ in range(32):
            self.stream_cursor, page = read_events(self.pub, PREFIX, self.stream_cursor)
            stream_rows.extend(page)
            if len(page) < 256: break
        for _, fields in stream_rows:
            event = json.loads(fields["data"])
            if fields["type"] == "log":
                # The shared durable queue can retain previous fixture boots.
                if self.state and event.get("boot") == self.state["boot"]:
                    self.logs.append(event)
            else: self.events[(fields["source"], event["seq"])] = event
        if not select.select([self.sub.sock], [], [], timeout)[0]:
            return
        msg = self.sub.read()
        if msg[0] != "message":
            return
        data = json.loads(msg[2])
        if msg[1].endswith(".log"):
            self.logs.append(data)
            return
        assert len(data.get("events", [])) <= 256
        self.raw_sizes[data["seq"]] = len(msg[2].encode("utf-8"))
        self.state = data
        self.frames.append(data)
        if os.environ.get("MGLIVE_STRESS_PREPARE") == "1":
            self.frames[:]=self.frames[-2:]
        for event in data.get("events", []):
            assert "time" in event and "seq" in event
            self.events[(data["boot"], event["seq"])] = event
        if os.environ.get("MGLIVE_STRESS_PREPARE") == "1" and len(self.events)>4096:
            self.events=dict(list(self.events.items())[-2048:])

    def until(self, predicate, timeout=20):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if predicate():
                return
            self.poll(min(1, max(0, end - time.monotonic())))
        raise AssertionError("Timed out awaiting engine observation")

    def command_interrupt(self):
        from test_command_delivery import interrupted
        return interrupted(self)

    def translation_config(self):
        from test_translation_config import run
        return run(self)

    def chat_translation(self):
        from test_chat_translation import run
        return run(self)

    def dense_report(self):
        from test_dense_report import run
        return run(self)

    def chat_translation_budget(self):
        from test_translation_budget import run
        return run(self)

    def chat_translation_stress(self):
        from test_chat_translation import run
        return run(self, stress=True)

    def command_boot(self):
        from test_command_delivery import boot_bound
        return boot_bound(self)

    def command_receipts(self):
        from test_command_delivery import receipts
        return receipts(self)

    def command_budget(self):
        from test_command_delivery import result_budget
        return result_budget(self)

    def command_accounting(self):
        from test_command_delivery import result_accounting
        return result_accounting(self)

    def command_expiry(self):
        from test_command_delivery import result_expiry
        return result_expiry(self)

    def command_version(self):
        from test_command_delivery import script_version
        return script_version(self)

    def watch_areas(self):
        from test_watch_areas import run
        return run(self)

    def trigger_probe(self):
        result=self.command(op="run_chunk",code='object t=CreateTrapAtLocation(0,Location(OBJECT_SELF,Vector(5.0,5.0,0.0),0.0));MgReturn(JsonDump(ObjectToJson(t)));',**{"return":True})
        return result

    def attribution(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=5,y=5,z=0,f=0)["obj"]
        answer=self.command(op="npc_say",target=actor,text="Attributed speech",by="Storyteller")
        self.until(lambda:any(e.get("text")=="Attributed speech" for e in self.matching("chat")))
        chat=next(e for e in self.matching("chat") if e.get("text")=="Attributed speech")
        assert answer["by"]==chat["by"]=="Storyteller" and chat["command_id"]==answer["id"]
        other=self.command(op="npc_say",target=actor,text="Unattributed speech")
        self.until(lambda:any(e.get("text")=="Unattributed speech" for e in self.matching("chat")))
        plain=next(e for e in self.matching("chat") if e.get("text")=="Unattributed speech")
        assert "by" not in plain and "by" not in other
        patrol=self.command(op="patrol",target=actor,points=[{"x":5,"y":5,"z":0},{"x":7,"y":5,"z":0}],loop=True,run=True,by="Director")
        self.until(lambda:any(e.get("status")=="reached" and e.get("command_id")==patrol["id"] for e in self.matching("patrol")))
        rows=[e for e in self.matching("patrol") if e.get("command_id")==patrol["id"]]
        assert rows and all(e.get("by")=="Director" for e in rows),rows
        self.command(op="stop_patrol",target=actor)
        damage=self.command(op="run_chunk",code='ApplyEffectToObject(DURATION_TYPE_INSTANT,EffectDamage(1),StringToObject("'+actor+'"));',by="Director")
        self.until(lambda:any(e.get("command_id")==damage["id"] for e in self.matching("damage")))
        hit=next(e for e in self.matching("damage") if e.get("command_id")==damage["id"])
        assert hit["by"]=="Director"
        killed=self.command(op="kill",target=actor,by="Director")
        self.until(lambda:any(e.get("command_id")==killed["id"] for e in self.matching("death")))
        death=next(e for e in self.matching("death") if e.get("command_id")==killed["id"])
        assert death["by"]=="Director"
        return {"speech":chat,"plain":plain,"patrol":rows,"damage":hit,"death":death,"results":[answer,other,patrol,damage,killed]}

    def capture_dialog(self):
        from test_dialogue_capture import run
        return run(self)

    def spawn_world(self):
        from test_spawn_world import run
        return run(self)

    def atmosphere(self):
        from test_atmosphere import run
        return run(self)

    def command(self, expect="ok", wire_ascii=False, **args):
        self.counter += 1
        ident = f"proof-{self.counter}"
        command = {"id":ident, "area":"events", **args}
        if command["area"] is None: command.pop("area")
        self.pub.send("PUBLISH", PREFIX + ".cmd", json.dumps({"v": 1, "script_version": 2, "boot": self.state["boot"], "prefix": PREFIX, "commands": [command]},ensure_ascii=wire_ascii))
        assert self.pub.read() > 0, "Game has not subscribed"
        self.until(lambda: any(r["id"] == ident for f in self.frames for r in f["results"]))
        result = next(r for f in self.frames for r in f["results"] if r["id"] == ident)
        if expect == "ok":
            assert result["ok"], {k:v for k,v in result.items() if k not in ["object_json","value"]}
        else:
            assert not result["ok"] and result["code"] == expect, {k:v for k,v in result.items() if k not in ["object_json","value"]}
        return result

    def chunk(self, code):
        return self.command(op="run_chunk", code=code)

    def matching(self, kind):
        return [e for (boot, _), e in self.events.items()
                if self.state and boot == self.state["boot"] and e["kind"] == kind]

    def teleport(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        chest,door=self.furniture()
        item=self.command(op="spawn",kind="item",r="nw_it_mpotion001",x=4,y=4,z=0,f=0)["obj"]
        def read(obj):
            # Run on the source area, addressing the object globally even after a jump.
            code='object o=StringToObject("'+obj+'"); vector p=GetPosition(o); json v=JsonObject(); v=JsonObjectSet(v,"valid",JsonBool(GetIsObjectValid(o))); v=JsonObjectSet(v,"area",JsonString(GetResRef(GetArea(o)))); v=JsonObjectSet(v,"x",JsonFloat(p.x)); v=JsonObjectSet(v,"y",JsonFloat(p.y)); v=JsonObjectSet(v,"z",JsonFloat(p.z)); v=JsonObjectSet(v,"f",JsonFloat(GetFacing(o))); v=JsonObjectSet(v,"held",JsonInt(GetLocalInt(o,"MGLIVE_HELD"))); v=JsonObjectSet(v,"patrol",JsonInt(GetLocalInt(o,"MGLIVE_PATROL_ACTIVE"))); MgReturn(JsonDump(v));'
            return self.command(op="run_chunk",code=code,**{"return":True})["value"]
        # Persistence through the bounded snapshot/recreation path.
        self.chunk('object c=StringToObject("'+chest+'"); SetName(c,"Teleport chest"); SetPlotFlag(c,TRUE); SetLocalInt(c,"MGET_MARK",42); CreateItemOnObject("nw_it_mpotion001",c);')
        # Reject a large saved-state capture without moving or deleting the original.
        self.chunk('object c=StringToObject("'+chest+'"); string big="x"; int i; for(i=0;i<19;i++) big+=big; SetLocalString(c,"MGET_LARGE",big);')
        oversized=self.native('JsonInt(GetStringLength(JsonDump(ObjectToJson(StringToObject("'+chest+'"),TRUE))))')
        assert oversized>262144,oversized
        self.command(expect="invalid",op="teleport",target=chest,area="events2",x=5,y=5,z=0)
        assert read(chest)["valid"] and read(chest)["area"]=="events"
        self.chunk('DeleteLocalString(StringToObject("'+chest+'"),"MGET_LARGE");')
        results=[]
        for kind,obj in [("creature",actor),("placeable",chest),("door",door),("item",item)]:
            before=read(obj)
            answer=self.command(op="teleport",target=obj,x=7,y=7,z=0,**({"f":90} if kind!="door" else {}))
            after=read(obj)
            assert after["area"]=="events" and abs(after["x"]-7)<(1 if kind=="creature" else 0.01) and abs(after["y"]-7)<(1 if kind=="creature" else 0.01),(kind,after)
            assert abs(after["f"]-(before["f"] if kind=="door" else 90))<0.2,(kind,after)
            assert answer["obj"]==obj and not answer["value"]["recreated"],answer
            results.append({"kind":kind,"before":before,"same_area":after,"result":answer})
        # Native SetFacing ignores doors; the proven same-area fallback recreates.
        rotated=self.command(op="teleport",target=door,x=8,y=8,z=0,f=270)
        assert rotated["value"]["recreated"] and not read(door)["valid"],rotated
        door=rotated["obj"]; assert abs(read(door)["f"]-270)<0.01
        results[2]["same_area_rotation"]={"result":rotated,"native":read(door)}
        self.command(op="patrol",target=actor,points=[{"x":10,"y":7,"z":0},{"x":7,"y":7,"z":0}],loop=True,run=False)
        assert read(actor)["patrol"]==1
        for row,obj in zip(results,[actor,chest,door,item]):
            answer=self.command(op="teleport",target=obj,area="events2",x=5,y=5,z=0,f=180)
            moved=answer["obj"];after=read(moved)
            assert after["area"]=="events2" and abs(after["x"]-5)<(1 if row["kind"]=="creature" else 0.01) and abs(after["y"]-5)<(1 if row["kind"]=="creature" else 0.01),(row["kind"],after)
            assert abs(after["f"]-180)<0.2,(row["kind"],after)
            assert after["patrol"]==0 and after["held"]==0,after
            if row["kind"]!="creature": assert moved!=obj and not read(obj)["valid"],answer
            else: assert moved==obj
            row.update(cross_area=after,cross_result=answer)
            if row["kind"]=="placeable":
                preserved=self.native('JsonArrayInsert(JsonArrayInsert(JsonArrayInsert(JsonArrayInsert(JsonArray(),JsonString(GetName(StringToObject("'+moved+'")))),JsonInt(GetLocalInt(StringToObject("'+moved+'"),"MGET_MARK"))),JsonBool(GetPlotFlag(StringToObject("'+moved+'")))),JsonBool(GetIsObjectValid(GetFirstItemInInventory(StringToObject("'+moved+'")))))')
                assert preserved==["Teleport chest",42,True,True],preserved
                row["preserved_name_local_plot_inventory"]=preserved
            self.until(lambda:any(a["area"]=="events2" and any(o["id"]==moved for key in ["cre","plc","doors","items"] for o in a.get(key,[])) for a in self.state["areas"]))
        for patch in [dict(x=-1),dict(x=20),dict(y=20),dict(x=1e100),dict(f=360),dict(f=-1)]:
            args=dict(target=actor,area="events2",x=4,y=4,z=0);args.update(patch)
            self.command(expect="invalid",op="teleport",**args)
        self.command(expect="not_found",op="teleport",target=actor,area="missing",x=4,y=4,z=0)
        self.command(expect="disabled",op="pc_teleport",target=actor,x=4,y=4,z=0)
        self.command(op="teleport",target=actor,area=None,x=8,y=8,z=0)
        assert read(actor)["area"]=="events2" and abs(read(actor)["f"]-180)<0.01
        alias=self.command(op="pc_teleport",target=actor,players=True,x=3,y=5,z=0,f=45)
        assert read(actor)["area"]=="events" and abs(read(actor)["f"]-45)<0.01,alias
        self.command(op="npc_move",target=actor,x=18,y=18,z=0,hold=True,timeout=60,run=False)
        assert read(actor)["held"]>0
        self.command(op="teleport",target=actor,area="events2",x=12,y=12,z=0)
        assert read(actor)["held"]==0 and read(actor)["patrol"]==0
        self.until(lambda:any(a["area"]=="events" and a.get("full") and not any(o["id"]==actor for o in a.get("cre",[])) for a in self.state["areas"]))
        self.command(op="run_chunk",code='object o=StringToObject("'+actor+'"); SetLocalInt(GetModule(),"MGET_INV",0); object it=CreateItemOnObject("nw_it_mpotion001",o); SetLocalString(GetModule(),"MGET_INV",ObjectToString(it));')
        inventory=self.native('JsonString(GetLocalString(GetModule(),"MGET_INV"))')
        self.command(expect="protected",op="teleport",target=inventory,x=4,y=4,z=0)
        return {"objects":results,"bounds_and_inventory_refused":True,"oversized_snapshot_bytes":oversized,"patrol_and_hold_cancelled":True,"alias_and_omitted_area":True,"limitation":"Positive PC teleport requires a connected owner client; headless proves the shared alias path on an NPC and missing-switch refusal, not a connected player jump."}

    def pc_skill_target(self):
        baseline = self.pc_skill_check()
        actor = self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(expect="disabled",op="skill_check",target=actor,skill=0,dc=15)
        self.command(expect="protected",op="skill_check",players=True,target=actor,skill=0,dc=15)
        self.command(expect="not_found",op="skill_check",players=True,target="7fffffff",skill=0,dc=15)
        for patch in [dict(x=3),dict(y=5),dict(radius=10),dict(target=12)]:
            args=dict(target=actor,skill=0,dc=15);args.update(patch)
            self.command(expect="invalid",op="skill_check",players=True,**args)
        self.command(op="run_script",script="mget_nui")
        window = self.native('GetLocalJson(GetModule(), "MGET_NUI_EMPTY")')
        assert window and "Success" in json.dumps(window) and '160' in json.dumps(window),window
        return {"baseline":baseline,"single_target_refusals":True,"empty_text_window":window,
                "limitation":"No connected PC: exact-target positive roll/NuiCreate token require owner client; no simulated PC claimed."}

    def silent_chat(self):
        actor = self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(op="run_script",script="mget_silent",target=actor)
        self.until(lambda: all(any(e.get("text")==name for e in self.matching("chat")) for name in ["silent-talk-proof","silent-shout-proof","audible-proof"]))
        found = [e for e in self.matching("chat") if e.get("text","").endswith("-proof")]
        for e in found:
            expected = {"silent-talk-proof":13,"silent-shout-proof":14,"audible-proof":1}[e["text"]]
            assert e["channel"] == expected and e["silent"] == (expected != 1),e
        self.command(op="kill",target=actor)
        self.until(lambda:any(e.get("text")=="NW_I_AM_DEAD" for e in self.matching("chat")))
        ai=next(e for e in self.matching("chat") if e.get("text")=="NW_I_AM_DEAD")
        assert ai["silent"] is True,ai
        return {"observed":found,"stock_death_ai":ai}

    def properties(self,actor):
        self.command(op="run_script",script="mget_props",target=actor)
        return self.native('GetLocalJson(OBJECT_SELF,"MGET_PROPERTIES")',actor)

    def object_row(self,oid,kind):
        return next((o for frame in reversed(self.frames) for area in frame["areas"] for o in area.get(kind,[]) if o["id"]==oid),None)

    def instance_appearance(self):
        chest = self.command(op="spawn",kind="placeable",r="mget_chest",x=3,y=5,z=0,f=0)["obj"]
        door = next(o["id"] for frame in reversed(self.frames) for area in frame["areas"] for o in area.get("doors",[]))
        def object_row(oid, kind):
            return next((o for frame in reversed(self.frames) for area in frame["areas"] for o in area.get(kind,[]) if o["id"]==oid),None)
        self.until(lambda: object_row(chest,"plc") is not None and object_row(door,"doors") is not None)
        before = [object_row(chest,"plc"),object_row(door,"doors")]
        snapshot=self.command(op="capture_object",target=door)["object_json"]
        fields={k:v for k,v in snapshot.items() if "generic" in k.lower() or "appear" in k.lower()}
        dump(OUT/"door-appearance-readback.json",fields)
        assert before[0]["app"]==4 and before[1]["app"]==1,(before,fields)
        self.command(op="run_script",script="mget_app",target=chest)
        self.until(lambda: object_row(chest,"plc").get("app")==5)
        after = object_row(chest,"plc")
        native = self.properties(chest)["app"]
        assert after["r"]==before[0]["r"] and native==5,after
        return {"blueprint_placeable_app":4,"before":before,"instance_after":after,"native":native,
                "limitation":"Authored cabinet/chest fixture proves template and instance can differ. Owner's running demo objects were not inspected or changed; client visual comparison remains owner's check."}

    def lootable(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(op="run_chunk",target=actor,code='object a=CreateItemOnObject("nw_it_mpotion001",OBJECT_SELF); object b=CreateItemOnObject("nw_it_mpotion002",OBJECT_SELF); SetDroppableFlag(a,TRUE); SetDroppableFlag(b,FALSE); SetLocalObject(GetModule(),"MGET_DROP_A",a); SetLocalObject(GetModule(),"MGET_DROP_B",b);')
        def flags():
            return self.native('JsonArrayInsert(JsonArrayInsert(JsonArray(),JsonBool(GetDroppableFlag(GetLocalObject(GetModule(),"MGET_DROP_A")))),JsonBool(GetDroppableFlag(GetLocalObject(GetModule(),"MGET_DROP_B"))))')
        assert self.native('JsonBool(GetIsObjectValid(GetLocalObject(GetModule(),"MGET_DROP_B")))')
        assert flags()==[True,False]
        changed=self.command(op="set_lootable",target=actor,lootable=False,no_drop=True)
        self.until(lambda:self.creature(actor).get("loot") is False)
        assert flags()==[False,False]
        restored=self.command(op="set_lootable",target=actor,lootable=True,no_drop=False)
        self.until(lambda:self.creature(actor).get("loot") is True)
        assert flags()==[True,False]
        self.command(op="set_lootable",target=actor,lootable=True,no_drop=True)
        self.chunk('SetDroppableFlag(GetLocalObject(GetModule(),"MGET_DROP_B"),TRUE);')
        self.command(op="set_lootable",target=actor,lootable=True,no_drop=False)
        assert flags()==[True,True], "Preserve a script's newer drop-enabled flag"
        self.command(expect="invalid",op="set_lootable",target=actor,lootable=1)
        self.command(op="kill",target=actor)
        self.until(lambda:self.creature(actor) and self.creature(actor).get("dead"))
        self.command(expect="dead",op="set_lootable",target=actor,lootable=False)
        return {"disabled":changed,"restored":restored,"initial_and_restored_flags":[True,False],"dead_refused":True}

    def placeable_appearance(self):
        actor=self.command(op="spawn",kind="placeable",r="mget_chest",x=3,y=5,z=0,f=0)["obj"]
        result=self.command(op="set_appearance",target=actor,row=5)
        self.until(lambda:self.object_row(actor,"plc") and self.object_row(actor,"plc").get("app")==5)
        native=self.properties(actor)["app"]
        assert native==5 and result["value"]["app"]==5
        self.command(expect="not_found",op="set_appearance",target=actor,row=65535)
        # Safe caller-driven replacement recipe, using the native snapshot and same appearance.
        snapshot=self.command(op="capture_object",target=actor)["object_json"]
        clone=self.command(op="spawn_json",json=snapshot,x=7,y=5,z=0,f=0)["obj"]
        self.until(lambda:self.object_row(clone,"plc") and self.object_row(clone,"plc").get("app")==5)
        return {"direct":result,"native":native,"spawn_json_clone":self.object_row(clone,"plc"),"limitation":"No connected client: direct appearance refresh unverified. Caller may capture, spawn_json, verify the new object, then explicitly destroy the old ID; references/actions are not migrated."}

    def static_flag(self):
        actor=self.command(op="spawn",kind="placeable",r="mget_chest",x=3,y=5,z=0,f=0)["obj"]
        observed=[]
        for value in [True,False]:
            result=self.command(op="set_static",target=actor,static=value)
            self.until(lambda:self.object_row(actor,"plc") and self.object_row(actor,"plc").get("static")==value)
            native=self.properties(actor)["static"]
            assert native==value and result["value"]["static"]==value
            observed.append(self.object_row(actor,"plc"))
        self.command(op="set_visual_transform",target=actor,scale=1.5)
        self.until(lambda:self.object_row(actor,"plc").get("vt",{}).get("scale")==1.5)
        return {"states":observed,"dynamic_transform":self.object_row(actor,"plc"),"client_limit":"NWNX documents that clients must re-enter the area to receive the static change."}

    def pc_force_bounds(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        ops=[dict(op="pc_message",text="Hello",mode="chat"),dict(op="pc_journal",quest="intro",state=1),dict(op="pc_take_item",resref="nw_it_mpotion001"),dict(op="pc_boot",reason="Proof"),dict(op="fade",direction="to_black",seconds=2),dict(op="play_sound",resref="as_pl_birds1")]
        for fields in ops:
            self.command(expect="disabled",target=actor,**fields)
            self.command(expect="protected",target=actor,players=True,**fields)
        return {"two_switches_and_real_PC_required":True,"ops":[o["op"] for o in ops],"limitation":"No connected private PC. Positive forced PC movement/attack, messages, journal, removal, boot, fade and sound require owner client acceptance; no fake PC or demo used."}

    def pc_dm_chat(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bandit001",x=3,y=5,z=0,f=0)["obj"]
        self.command(expect="disabled",op="pc_message",target=actor,text="Reply",mode="dm")
        self.command(expect="protected",op="pc_message",target=actor,text="Reply",mode="dm",players=True)
        self.command(op="run_script",script="mget_dm",target=actor)
        self.until(lambda:any(e.get("text")=="to-dm-proof" for e in self.matching("chat")))
        dm=next(e for e in self.matching("chat") if e.get("text")=="to-dm-proof")
        ai=next(e for e in self.matching("chat") if e.get("text")=="silent-ai-proof")
        assert dm["id"]==actor and dm["channel"]==30 and dm["to_dm"] and not dm["silent"] and dm["audience"]=="dm",dm
        assert ai["channel"]==14 and ai["silent"] and not ai["to_dm"],ai
        reply=self.native('GetLocalJson(OBJECT_SELF,"MGET_DM_REPLY")',actor)
        assert reply["text"]=="DM: Private reply proof" and reply["recipient"]==actor and reply["native"]=="SendMessageToPC"
        return {"dm_channel":dm,"npc_silent_channel":ai,"reply_standin":reply,"limitation":"No connected PC/DM. Actual PC channel14 GetIsPC branch, delivery and recipient client display require owner test; private helper proves native call and exact text, not delivery."}

    def pc_reply(self):
        standin=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        nearest=self.command(op="spawn",kind="creature",r="nw_bartender",x=5,y=5,z=0,f=0)["obj"]
        chosen=self.command(op="spawn",kind="creature",r="nw_bartender",x=8,y=5,z=0,f=0)["obj"]
        refused=[self.command(expect="disabled",op="reply_to_player",player=standin,text="hello",area=None),
                 self.command(expect="protected",op="reply_to_player",player=standin,text="hello",players=True,area=None),
                 self.command(expect="invalid",op="reply_to_player",player="bad-id",text="hello",players=True,area=None),
                 self.command(expect="invalid",op="reply_to_player",player=standin,text=" ",players=True,area=None)]
        self.chunk(f'SetLocalObject(StringToObject("{standin}"),"MGET_REPLY_AS",StringToObject("{chosen}"));')
        self.command(op="run_script",script="mget_reply",target=standin)
        rows=self.native('GetLocalJson(OBJECT_SELF,"MGET_REPLIES")',standin)
        assert rows[0]["ok"] and rows[0]["value"]["speaker"]==nearest and rows[0]["value"]["channel"]==1,rows
        assert rows[1]["ok"] and rows[1]["value"]["speaker"]==chosen and rows[1]["value"]["channel"]==1,rows
        assert not rows[2]["ok"] and rows[2]["value"]["route"]=="dm" and rows[2]["value"]["channel"]==30 and not rows[2]["value"]["submitted"],rows
        self.until(lambda:all(any(e.get("text")==text and e["id"]==who for e in self.matching("chat")) for text,who in [("reply-nearest-proof",nearest),("reply-explicit-proof",chosen)]))
        return {"refusals":refused,"native_results":rows,"chat":[e for e in self.matching("chat") if e.get("text","").startswith("reply-")],"limitation":"No connected PC. Production gate rejects NPC substitute; exact shared selector/talk delivery succeeds. Targeted native DM30 correctly refuses the non-client stand-in; connected-PC DM display and module speaker fallback remain client checks."}

    def animation_replacement(self):
        npc=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        chest,door=self.furniture()
        item=self.command(op="spawn",kind="item",r="nw_it_mpotion001",x=5,y=5,z=0,f=0)["obj"]
        results=[]
        for kind,target in [("creature",npc),("placeable",chest),("door",door),("item",item)]:
            result=self.command(op="replace_object_animation",target=target,old="walk",new="run")
            rows=result["value"]["state"]["value"]["AnimationReplace"]["value"]
            assert any(r["OldAnimation"]["value"]=="walk" and r["NewAnimation"]["value"]=="run" for r in rows),result
            reset=self.command(op="replace_object_animation",target=target,old="walk",reset=True)
            rows=reset["value"].get("state",{}).get("value",{}).get("AnimationReplace",{}).get("value",[])
            assert not any(r["OldAnimation"]["value"]=="walk" for r in rows),reset
            results.append({"kind":kind,"set":result,"reset":reset})
        return {"native_animation_replacements":results}

    def chat(self):
        actor = self.command(op="spawn", kind="creature", r="nw_bandit001", x=9, y=9, z=0, f=0)["obj"]
        self.command(op="npc_say", target=actor, text='events proof "quoted" \\ slash')
        self.until(lambda: any(e.get("text") == 'events proof "quoted" \\ slash' for e in self.matching("chat")))
        found = next(e for e in self.matching("chat") if e["text"].startswith("events proof"))
        assert found["id"] == actor and found["name"] and found["area"] == "events"
        # Native NWNX send routes cover all defined channel numbers without filtering.
        channels = [1, 2, 3, 4, 5, 6, 14, 17, 18, 19, 20, 22, 30]
        self.command(op="run_script", script="mget_chat")
        self.until(lambda: all(any(e.get("text") == f"channel-{c}" and e["channel"] == c for e in self.matching("chat")) for c in channels))
        # A 300-line native burst drains across bounded flushes without loss.
        self.chunk('SetLocalInt(GetModule(), "MGET_CHAT_BURST", TRUE);')
        self.command(op="run_script", script="mget_chat")
        self.until(lambda:any(e.get("text")=="burst-299" for e in self.matching("chat")))
        assert self.state.get("dropped",0)==0
        assert [e["text"] for e in self.matching("chat") if e.get("text","").startswith("burst-")]==[f"burst-{i}" for i in range(300)]
        # Eight separate VM entries, with zero state subscribers: all 2400
        # now wait in Redis, independently of state publication/subscriber count.
        self.sub.close()
        for n in range(8):
            self.pub.send("PUBLISH",PREFIX+".cmd",json.dumps({"v":1,"script_version":2,"boot":self.state["boot"],"prefix":PREFIX,"commands":[{"id":f"overflow-{n}","op":"run_script","area":"events","script":"mget_chat"}]}))
            assert self.pub.read()>0
            time.sleep(.2)
        time.sleep(2)
        self.sub=Redis();self.sub.send("SUBSCRIBE",PREFIX+".state",PREFIX+".log");self.sub.read();self.sub.read()
        self.until(lambda:any(e.get("text")=="burst-2699" for e in self.matching("chat")),timeout=30)
        retained=[e for e in self.matching("chat") if e.get("text","").startswith("burst-") and int(e["text"][6:])>=300]
        assert [e["text"] for e in retained]==[f"burst-{i}" for i in range(300,2700)]
        assert self.state["dropped"]==0
        burst={"dropped":self.state["dropped"],"events":retained}
        seqs = [e["seq"] for (boot, _), e in self.events.items() if boot == self.state["boot"]]
        assert seqs == sorted(set(seqs))
        self.command(op="run_script", script="mglive_probe")
        self.sub.close()
        # Retain a later native speech event across ticks with zero subscribers.
        self.pub.send("PUBLISH", PREFIX + ".cmd", json.dumps({"v": 1, "script_version": 2, "boot": self.state["boot"], "prefix": PREFIX, "commands": [{
            "id": "no-listener", "area": "events", "op": "npc_say", "target": actor,
            "text": "retained-without-listener"}]}))
        assert self.pub.read() > 0
        time.sleep(3)
        self.sub = Redis()
        self.sub.send("SUBSCRIBE", PREFIX + ".state", PREFIX + ".log")
        self.sub.read()
        self.sub.read()
        self.until(lambda: any(e.get("text") == "retained-without-listener" for e in self.matching("chat")))
        return {"channels": channels, "speaker": actor, "native_speech": found,
                "retained_without_subscriber": True,
                "subscriber_gap": {"count": len(retained), "dropped": burst["dropped"], "first": burst["events"][0], "last": burst["events"][-1]},
                "limitation": "All channel hooks tested with downstream native delivery skipped in the fixture; native NPC talk delivered normally. Real player tells/target resolution need a client join."}


    def combat(self):
        spawn = lambda r, x, f: self.command(op="spawn", kind="creature", r=r, x=x, y=5, z=0, f=f)["obj"]
        # y=5, clear of the fixture door at the area centre (10,10): fighters near or
        # across it never started an attack round (moving the door fixed that too).
        bandit, victim = spawn("nw_bandit001", 3, 0), spawn("nw_bartender", 6, 180)
        self.command(op="npc_attack", target=bandit, victim=victim)
        self.until(lambda: any(e["id"] == bandit and e["target"] == victim for e in self.matching("attack")), 30)
        self.until(lambda: any(e["target"] == victim and e["amount"] > 0 for e in self.matching("damage")), 60)
        self.until(lambda: any(e["id"] == victim and e["killer"] == bandit for e in self.matching("death")), 60)
        attack = next(e for e in self.matching("attack") if e["id"] == bandit)
        death = next(e for e in self.matching("death") if e["id"] == victim)
        assert len([e for e in self.matching("death") if e["id"] == victim]) == 1, "one death per creature"
        self.command(op="destroy", target=bandit)

        spell = self.spell()
        # The victim remains across subsequent ticks while the second pair casts.
        assert len([e for e in self.matching("death") if e["id"] == victim]) == 1, "death repeated on a later tick"
        return {"attack": attack, "death": death,
                "damage": next(e for e in self.matching("damage") if e["target"] == victim),
                "spell": spell, "limitation": "player_join/leave need a client join; not exercised here."}

    def spell(self, projectile=False):
        spawn = lambda r, x, f: self.command(op="spawn", kind="creature", r=r, x=x, y=5, z=0, f=f)["obj"]
        caster, mark = spawn("nw_bandit001", 6, 0), spawn("nw_bartender", 14, 180)
        self.chunk(f'SetLocalObject(GetModule(), "MGET_TARGET", StringToObject("{mark}")); '
                   f'SetLocalInt(GetModule(), "MGET_CAST_PROJECTILE", {int(projectile)}); '
                   f'SetImmortal(StringToObject("{mark}"), TRUE);')
        self.command(op="npc_attack", target=caster, victim=mark, hold=True)
        self.until(lambda: any(e["id"] == caster and e["target"] == mark for e in self.matching("attack")), 30)
        self.command(op="run_script", target=caster, script="mget_cast")
        self.until(lambda: any(e["id"] == caster and e["spell"] == 107 for e in self.matching("spell")), 30)
        spell = next(e for e in self.matching("spell") if e["id"] == caster)
        assert spell["target"] == mark, spell
        return spell

    def delta(self):
        return self.rust_api("delta")

    def chunk_return(self):
        return self.rust_api("chunk_return")

    def rust_api(self, mode="ops"):
        # The production Rust service owns both transports; Python only starts
        # the isolated engine, watches its raw frames, and binds the evidence.
        binary = REPO / "target/debug/examples/live_ops_proof.exe"
        assert binary.is_file(), "Build mg-live --example live_ops_proof in the shared target first"
        guard = OUT / "no-native-game"
        assert not guard.exists()
        env = dict(os.environ, NWN_ROOT=win(guard))
        env["WSLENV"] = env.get("WSLENV", "") + ":NWN_ROOT"
        capture = Redis()
        capture.send("SUBSCRIBE", PREFIX + ".cmd")
        capture.read()
        proc = subprocess.Popen([str(binary), self.state["boot"], mode], stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, env=env)
        published = []
        try:
            deadline = time.monotonic() + 180
            while proc.poll() is None:
                assert time.monotonic() < deadline, "Private Rust API proof timed out"
                self.poll(.1)
                if select.select([capture.sock], [], [], 0)[0]:
                    message = capture.read()
                    if message[0] == "message":
                        assert message[2].isascii(), "Rust publisher sent non-ASCII JSON"
                        published.append(json.loads(message[2]))
            stdout, stderr = proc.communicate(timeout=5)
            assert proc.returncode == 0, stderr.decode("utf-8", errors="replace")[-5000:]
            result = json.loads(stdout.decode("utf-8"))
            assert result["passed"] and result["boot"] == self.state["boot"]
            self.until(lambda: self.state["seq"] >= result["final_seq"])
            commands = [c for envelope in published for c in envelope["commands"]]
            assert commands and all("players" not in c for c in commands)
            assert not any(c["op"] == "pc_give_gold" for c in commands)
            if mode == "chunk_return":
                raw={r["id"]:r for frame in self.frames for r in frame.get("results",[])}
                for answer in result["results"]:
                    assert answer["id"] in raw
                    assert raw[answer["id"]].get("value")==answer.get("value")
            elif mode == "delta":
                assert any(c["op"]=="resync" for c in commands), "Moonglow did not request recovery"
                full=[f for f in self.frames if "base_seq" not in f and any(len(a.get("plc",[]))>=599 for a in f.get("areas",[]))]
                delta=[f for f in self.frames if "base_seq" in f and f["seq"]>full[0]["seq"] and not f.get("results") and not f.get("events")]
                assert len(full)>=2 and delta
                assert any(result["second"] in a.get("removed",[]) for f in delta+self.frames for a in f.get("areas",[]))
                assert any(o["id"]==result["first"] and o["name"]=="Delta changed" for f in delta+self.frames for a in f.get("areas",[]) for o in a.get("plc",[]))
                full_bytes=max(self.raw_sizes[f["seq"]] for f in full)
                delta_bytes=[self.raw_sizes[f["seq"]] for f in delta]
                result["bytes"]={"full":full_bytes,"delta_mean":sum(delta_bytes)/len(delta_bytes),"delta_samples":len(delta_bytes),"periodic_full_seqs":[f["seq"] for f in full]}
                assert result["bytes"]["delta_mean"] < full_bytes*.1, result["bytes"]
            elif mode in ("chat_latency", "chat_latency_before"):
                assert len(result["samples"])==20
                assert len({s["seq"] for s in result["samples"]})==20
            elif mode == "mixed_speech":
                from urllib.parse import unquote_to_bytes
                chats=[e for e in self.matching("chat") if e.get("text_bytes")]
                for phrase in ["Naprawdę sama widziałam", "Teraz już każdy o tym wie"]:
                    for encoding in ["utf-8", "cp1250"]:
                        assert any(unquote_to_bytes(e["text_bytes"].replace("+"," "))==phrase.encode(encoding) for e in chats)
                result["raw_chat"] = chats
            elif mode == "module_polish":
                from urllib.parse import unquote_to_bytes
                chat=[e for e in self.matching("chat") if e.get("text_bytes")]
                source=polish_tlk_bytes()
                assert any(unquote_to_bytes(e["text_bytes"].replace("+"," "))==source for e in chat)
                assert any(unquote_to_bytes(e["text_bytes"].replace("+"," ")).startswith("Naprawd\u0119".encode()) for e in chat)
                result["cp1250_dialogue_bytes"]=source.hex()
            else:
                assert {"freeze", "set_faction", "set_visual_transform", "set_name", "npc_say"} <= {c["op"] for c in commands}
                npc, plc = result["npc"], result["placeable"]
                creatures = [o for frame in self.frames for a in frame["areas"] for o in a.get("cre", []) if o["id"] == npc]
                objects = [o for frame in self.frames for a in frame["areas"] for o in a.get("plc", []) if o["id"] == plc]
                assert any(o.get("frozen") for o in creatures)
                assert any(o.get("faction") == 3 for o in creatures)
                assert any(o.get("vt") == result["transformed"]["vt"] for o in objects)
                assert any(e.get("text") == result["polish_name"]["name"] for e in self.matching("chat"))
            result.update(binary_sha256=sha(binary),
                          source_sha256=sha(REPO / "crates/mg-live/examples/live_ops_proof.rs"),
                          published=published, wire_ascii=True, server_players=mode=="ops",
                          raw_engine_reports_confirmed=True, helper_exited=True)
            return result
        finally:
            capture.close()
            if proc.poll() is None:
                proc.terminate()  # Only the helper child just started above.
                proc.wait(timeout=8)

    def creature(self, ident):
        return next((o for a in self.state["areas"] for o in a["cre"] if o["id"] == ident), None)

    def object(self, ident):
        # Non-creatures occur only in full reports (every five ticks).
        for frame in reversed(self.frames):
            for area in frame["areas"]:
                if area["full"]:
                    return next((o for key in ["cre","plc","doors","items"] for o in area.get(key,[]) if o["id"]==ident),None)
        return None

    def furniture(self):
        chest=self.command(op="spawn",kind="placeable",r="mget_chest",x=5,y=5,z=0,f=0)["obj"]
        self.until(lambda:any(o["r"]=="mget_door" for a in self.state["areas"] for o in a.get("doors",[])))
        door=next(o["id"] for a in self.state["areas"] for o in a.get("doors",[]) if o["r"]=="mget_door")
        self.until(lambda:self.object(chest) is not None)
        return [chest,door]

    def native(self, expression, target=None):
        return self.command(op="run_chunk",code="MgReturn(JsonDump("+expression+"));",**({"target":target} if target else {}),**{"return":True})["value"]

    def tile(self):
        def tiles():
            return self.command(op="run_chunk",code='object a=OBJECT_SELF; json rows=JsonArray(); int y; int x; for(y=0;y<2;y++) for(x=0;x<2;x++) {location l=Location(a,Vector(IntToFloat(x)*10.0+5.0,IntToFloat(y)*10.0+5.0,0.0),0.0); json r=JsonObjectSet(JsonObject(),"id",JsonInt(GetTileID(l))); r=JsonObjectSet(r,"orientation",JsonInt(GetTileOrientation(l))); r=JsonObjectSet(r,"height",JsonInt(GetTileHeight(l))); rows=JsonArrayInsert(rows,r);} MgReturn(JsonDump(rows));',**{"return":True})["value"]
        before=tiles();assert all(r["id"]==50 for r in before),before
        answer=self.command(op="set_tile",x=1,y=1,tile_id=51,orientation=1,height=1)
        after=tiles()
        assert after[:3]==before[:3] and after[3]=={"id":51,"orientation":1,"height":1},after
        self.until(lambda:any(e.get("tile_id")==51 and e["x"]==1 and e["y"]==1 for e in self.matching("tile")))
        event=next(e for e in self.matching("tile") if e.get("tile_id")==51)
        assert answer["value"]["flags"]==7 and event["area"]=="events",(answer,event)
        for fields in [dict(x=2),dict(x=-1),dict(orientation=4),dict(height=256),dict(tile_id=4294967346)]:
            args=dict(x=1,y=1,tile_id=50,orientation=0,height=0);args.update(fields)
            self.command(expect="invalid",op="set_tile",**args)
        self.command(expect="not_found",op="set_tile",x=1,y=1,tile_id=65535,orientation=0,height=0)
        self.command(op="set_tile",x=1,y=1,tile_id=50,orientation=0,height=0)
        restored=tiles();assert restored==before,restored
        return {"before":before,"after":after,"event":event,"result":answer,"restored":restored,"limitation":"Native server geometry fields proven, no connected-client terrain rendering proof."}

    def patrol(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        points=[{"x":6,"y":5,"z":0},{"x":6,"y":8,"z":0},{"x":3,"y":5,"z":0}]
        self.command(op="patrol",target=actor,points=points,loop=True,run=False)
        def arrivals():return [e for e in self.matching("patrol") if e["id"]==actor and e["status"]=="reached"]
        self.until(lambda:len(arrivals())>=6,50)
        observed=arrivals()[:6]
        assert [(e["point"],e["lap"]) for e in observed]==[(0,0),(1,0),(2,0),(0,1),(1,1),(2,1)],observed
        for e in observed:
            p=points[e["point"]];assert sum((p[k]-e["position"][k])**2 for k in "xy") <= .61**2,e
        self.command(op="npc_move",target=actor,x=2,y=5,z=0,hold=True)
        self.until(lambda:abs(self.creature(actor)["x"]-2)<.6 and abs(self.creature(actor)["y"]-5)<.6)
        stopped_by_order=self.creature(actor)
        assert self.native('JsonInt(GetLocalInt(OBJECT_SELF,"MGLIVE_PATROL_ACTIVE"))',actor)==0
        count=len(arrivals());seq=self.state["seq"]
        self.until(lambda:self.state["seq"]>=seq+4)
        assert len(arrivals())==count
        self.command(op="patrol",target=actor,points=[{"x":6,"y":5,"z":0}],loop=True,run=True)
        self.command(op="stop_patrol",target=actor)
        assert self.native('JsonBool(GetCommandable(OBJECT_SELF) && !GetLocalInt(OBJECT_SELF,"MGLIVE_PATROL_ACTIVE"))',actor)
        self.command(op="patrol",target=actor,points=[{"x":3,"y":5,"z":0}],loop=False,run=True)
        self.until(lambda:len(arrivals())>count)
        assert self.native('JsonBool(GetCommandable(OBJECT_SELF) && !GetLocalInt(OBJECT_SELF,"MGLIVE_PATROL_ACTIVE"))',actor)
        self.command(op="patrol",target=actor,points=[{"x":6,"y":5,"z":0}],loop=True)
        self.command(op="freeze",target=actor,seconds=2)
        assert self.native('JsonInt(GetLocalInt(OBJECT_SELF,"MGLIVE_PATROL_ACTIVE"))',actor)==0
        self.command(op="unfreeze",target=actor)
        for bad in [[],points*6,[{"x":True,"y":0,"z":0}]]:self.command(expect="invalid",op="patrol",target=actor,points=bad)
        return {"freeze_cancelled":True,"two_laps":observed,"new_move":stopped_by_order,"stop_restored_commandability":True,"one_shot_completed":True}

    def patrol_obstacle(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        # Destination is on the leg and physically occupied by a native chest.
        blocker=self.command(op="spawn",kind="placeable",r="mget_chest",x=10,y=5,z=0,f=0)["obj"]
        points=[{"x":10,"y":5,"z":0},{"x":14,"y":5,"z":0}]
        self.command(op="patrol",target=actor,points=points,loop=True,run=False)
        start=time.monotonic()
        while time.monotonic()-start<12: self.poll(.25)
        blocked=dict(self.creature(actor))
        distance=lambda:((self.creature(actor)["x"]-10)**2+(self.creature(actor)["y"]-5)**2)**.5
        events=lambda:[e for e in self.matching("patrol") if e["id"]==actor]
        assert not any(e["status"]=="reached" for e in events()), {"unexpected_arrival":events(),"position":blocked}
        assert distance()>.6,blocked
        baseline=os.environ.get("MGLIVE_PATROL_BASELINE")=="1"
        # The old implementation abandons the leg after 120s. The repaired one
        # must remain alive beyond that cutoff and recover without a new order.
        while time.monotonic()-start<126: self.poll(.25)
        # Read from the area: a run_chunk targeted at the actor is a new order
        # and deliberately cancels its patrol before the read executes.
        active=self.native(f'JsonInt(GetLocalInt(StringToObject("{actor}"),"MGLIVE_PATROL_ACTIVE"))')
        retries=self.native(f'JsonInt(GetLocalInt(StringToObject("{actor}"),"MGLIVE_PATROL_RETRIES"))')
        if baseline: assert active==0,active
        else: assert active>0 and retries>=2 and any(e["status"]=="blocked" for e in events()),events()
        self.command(op="destroy",target=blocker)
        removed=time.monotonic()
        while time.monotonic()-removed<12 and not any(e["status"]=="reached" for e in events()):self.poll(.25)
        if baseline:
            assert not any(e["status"]=="reached" for e in events())
        else:
            self.until(lambda:len([e for e in events() if e["status"]=="reached"])>=4,35)
            assert any(e["status"]=="resumed" for e in events()),events()
            self.command(op="stop_patrol",target=actor)
            count=len(events()); seq=self.state["seq"]
            self.until(lambda:self.state["seq"]>=seq+5)
            assert len(events())==count and self.native('JsonBool(GetCommandable(OBJECT_SELF))',actor)
        return {"baseline":baseline,"patrol_source_sha256":sha(OUT/"module/mglive_patrol.nss"),"actor":actor,"blocker":blocker,"blocked_position":blocked,"active_after_126s":active,"retries":retries,"events":events(),"after_removal":self.creature(actor)}

    def skill_check_disabled(self):
        return self.command(expect="disabled",op="skill_check",players=True,x=3,y=5,radius=30,skill=0,dc=15)

    def restore_appearance(self):
        observed=[]
        for kind,resref,changed in [("creature","nw_bartender",7),("placeable","mget_chest",5)]:
            oid=self.command(op="spawn",kind=kind,r=resref,x=3,y=3,z=0,f=0)["obj"]
            self.until(lambda:self.object(oid) is not None)
            first=self.object(oid); original=first["app"]
            assert original==first["app_original"] and original!=changed
            self.command(op="set_appearance",target=oid,row=changed)
            self.until(lambda:self.object(oid)["app"]==changed)
            during=self.object(oid); assert during["app_original"]==original
            self.command(op="set_appearance",target=oid,row=original)
            self.until(lambda:self.object(oid)["app"]==original)
            after=self.object(oid)
            assert after["app_original"]==original
            field="Appearance_Type" if kind=="creature" else "Appearance"
            assert self.native(f'JsonObjectGet(JsonObjectGet(ObjectToJson(StringToObject("{oid}")),"{field}"),"value")')==original
            observed.append({"kind":kind,"first":first,"changed":during,"restored":after})
        return {"objects":observed,"note":"First-report baseline, not guessed blueprint; client rendering remains owner check."}

    def drop_flags(self):
        template=os.environ.get("MGLIVE_DROP_TEMPLATE")
        if template:
            actor=self.command(op="spawn_json",json=json.loads(Path(template).read_text()),x=3,y=3,z=0,f=0)["obj"]
            inv=self.command(op="view_inventory",target=actor)["value"]
            item=next(r["id"] for r in inv["items"] if r["resref"]=="nw_it_mpotion001")
        else:
            actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
            item=self.command(op="give_item",target=actor,resref="nw_it_mpotion001")["obj"]
            self.chunk(f'SetDroppableFlag(StringToObject("{item}"),FALSE);')
        self.chunk(f'SetLocalInt(StringToObject("{item}"),"MGET_ITEM",73);')
        inv=self.command(op="view_inventory",target=actor)["value"]
        potion=next(r for r in inv["items"] if r["id"]==item)
        assert potion["droppable"] is False and potion["plot"] is False,potion
        if os.environ.get("MGLIVE_DROP_REPRO"):
            refused=self.command(expect="protected",op="drop_item",target=actor,item=item)
            return {"inventory":potion,"refused":refused}
        answer=self.command(op="drop_item",target=actor,item=item)
        ground_id=answer["obj"]
        self.until(lambda:self.object(ground_id) is not None)
        ground=self.object(ground_id);person=self.creature(actor)
        assert abs(ground["x"]-person["x"])<.01 and abs(ground["y"]-person["y"])<.01
        assert self.native(f'JsonBool(GetIsObjectValid(GetItemPossessor(StringToObject("{ground_id}"))))') is False
        assert self.native(f'JsonBool(GetDroppableFlag(StringToObject("{ground_id}")))') is False
        assert self.native(f'JsonInt(GetLocalInt(StringToObject("{ground_id}"),"MGET_ITEM"))')==73
        assert self.native(f'JsonBool(GetIsObjectValid(StringToObject("{item}")))') is False
        plot=self.command(op="give_item",target=actor,resref="nw_it_mpotion001")["obj"]
        self.chunk(f'SetPlotFlag(StringToObject("{plot}"),TRUE);')
        self.command(expect="protected",op="drop_item",target=actor,item=plot)
        return {"inventory":potion,"result":answer,"ground":ground,"flags_and_locals_preserved":True,"plot_refused":True}

    def drop_routine_probe(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        item=self.command(op="give_item",target=actor,resref="nw_it_mring001")["obj"]
        self.chunk(f'SetLocalInt(StringToObject("{item}"),"MGET_ITEM",73); SetLocalObject(StringToObject("{item}"),"MGET_OWNER",StringToObject("{actor}")); SetCommandable(FALSE,StringToObject("{actor}"));')
        # Reproduce the previous drop_item implementation on this stand-in.
        queued=self.command(op="run_chunk",code=f'object actor=StringToObject("{actor}"); AssignCommand(actor,ClearAllActions(TRUE)); AssignCommand(actor,ActionPutDownItem(StringToObject("{item}")));')
        seq=self.state["seq"]; self.until(lambda:self.state["seq"]>=seq+3)
        holder=self.native(f'JsonString(ObjectToString(GetItemPossessor(StringToObject("{item}"))))')
        assert holder==actor, "legacy queued drop unexpectedly succeeded"
        before=self.command(op="view_inventory",target=actor)["value"]
        answer=self.command(op="drop_item",target=actor,item=item)
        copy=answer["obj"]
        self.until(lambda:self.object(copy) is not None)
        after=self.command(op="view_inventory",target=actor)["value"]
        assert all(r["id"]!=item for r in after["items"])
        assert answer["value"]=={"previous":item,"item":copy,"queued":False}
        assert copy!=item and self.native(f'JsonBool(GetIsObjectValid(StringToObject("{item}")))') is False
        assert self.native(f'JsonInt(GetLocalInt(StringToObject("{copy}"),"MGET_ITEM"))')==73
        assert self.native(f'JsonString(ObjectToString(GetLocalObject(StringToObject("{copy}"),"MGET_OWNER")))')==actor
        assert self.native(f'JsonBool(GetItemPossessor(StringToObject("{copy}"))==OBJECT_INVALID)') is True
        ground=self.object(copy); person=self.creature(actor)
        assert abs(ground["x"]-person["x"])<.01 and abs(ground["y"]-person["y"])<.01
        assert self.native(f'JsonBool(GetCommandable(StringToObject("{actor}")))') is False
        return {"old_queued_result":queued,"old_still_held":holder,"before":before,"result":answer,"after":after,"ground":ground,"locals_preserved":True,"actor_commandable_unchanged":True,
                "limitation":"Controlled uncommandable actor reproduces false-success queued drop; runtime ground copy changes item identity. No mutation of the running demo."}

    def focus_chat(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        other=self.command(op="spawn",kind="creature",r="nw_bartender",x=4,y=5,z=0,f=0)["obj"]
        self.chunk(f'SetLocalObject(GetModule(),"MGET_FOCUS_LISTENER",StringToObject("{other}"));')
        self.command(op="run_script",script="mget_focuschat",target=actor)
        channels=[1,2,3,4,5,6,14,17,18,19,20,22,30]
        self.until(lambda:all(any(e.get("text")==f"focus-channel-{c}" for e in self.matching("chat")) for c in channels))
        self.until(lambda:any(e.get("text")=="focus-native-whisper" for e in self.matching("chat")))
        events=[e for e in self.matching("chat") if e.get("text","").startswith("focus-")]
        for e in events:
            assert e["id"]==actor and e["area"]=="events",e
            assert abs(e["position"]["x"]-3)<.01 and abs(e["position"]["y"]-5)<.01,e
            assert e["recipient"]==e["target"] is None,e
            expected={1:"vicinity",2:"server",3:"vicinity",4:"unresolved",5:"server",6:"party",14:"ai",17:"vicinity",18:"server",19:"vicinity",20:"unresolved",22:"party",30:"dm"}
            assert e["audience"]==expected[e["channel"]],e
        rejected={key:self.native(f'JsonInt(GetLocalInt(GetModule(),"MGET_FOCUS_NONPC_{key}"))') for key in ["TELL","DM"]}
        assert rejected=={"TELL":0,"DM":0},rejected
        assert not any(e.get("text","").startswith("not-a-player-") for e in self.matching("chat"))
        return {"events":events,"nonplayer_direct_send_return":rejected,"limitation":"Recipient resolution requires a real CNWSPlayer connection. NPC stand-ins cannot prove tells/DM delivery; null remains explicit."}

    def focus_dialog(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        other=self.command(op="spawn",kind="creature",r="nw_bartender",x=4,y=5,z=0,f=0)["obj"]
        self.chunk(f'SetLocalObject(GetModule(),"MGET_FOCUS_LISTENER",StringToObject("{other}"));')
        self.command(op="run_script",script="mget_dlgline",target=actor)
        assert not self.matching("dialog"),"out-of-dialog callback must not log a line"
        snapshots=[]
        for mode in [0,1,2]:
            self.chunk('SetLocalInt(GetModule(),"MGET_FOCUS_ONELINER",'+str(int(mode==2))+'); SetLocalInt(GetModule(),"MGET_FOCUS_EXPLICIT",'+str(int(mode==1))+');')
            self.command(op="run_script",script="mget_focstart",target=actor)
            end=time.monotonic()+4
            while time.monotonic()<end:self.poll(.2)
            snapshot={key:self.native(f'JsonInt(GetLocalInt(GetModule(),"MGET_FOCUS_{key}"))') for key in ["ACTIONS","CAPTURED","TYPE","SCRIPT","CONDITIONS","COND_CAPTURED"]}
            snapshot["param"]=self.native('JsonString(GetLocalString(GetModule(),"MGET_FOCUS_PARAM"))')
            snapshot["objects"]={key:self.native(f'JsonString(ObjectToString(GetLocalObject(GetModule(),"MGET_FOCUS_{key}")))') for key in ["OWNER","LAST","PC"]}
            snapshots.append(snapshot)
        rows=self.matching("dialog")
        assert [e["node_type"] for e in rows]==["entry","reply","entry","reply"],rows
        assert all(e["dialog"]=="mget_focus" and e["node"]==0 and e["owner"]==actor for e in rows),rows
        assert rows[0]["speaker"]==actor and rows[0]["listener"] is None
        assert rows[1]["speaker"] is None and rows[1]["listener"]==actor
        assert rows[2]["speaker"]==actor and rows[2]["listener"]==other
        assert rows[3]["speaker"]==other and rows[3]["listener"]==actor
        from urllib.parse import unquote_to_bytes
        expected=["Naprawd\u0119: \u0105\u0119\u0142\u0144\u00f3\u015b\u017c\u017a", "Odpowied\u017a: \u0105\u0119\u0142\u0144\u00f3\u015b\u017c\u017a"]
        decoded=[unquote_to_bytes(e["text_bytes"].replace("+"," ")).decode("cp1250") for e in rows]
        assert decoded==expected*2,decoded
        assert all(s["COND_CAPTURED"]==0 and s["param"]=="original-param" for s in snapshots),snapshots
        assert snapshots[2]["CAPTURED"]==snapshots[1]["CAPTURED"]==4,snapshots
        return {"actor":actor,"listener_standin":other,"snapshots":snapshots,"dialog_events":rows,"decoded_text":decoded,
                "chat_events":self.matching("chat"),"limitation":"NPC-only dialog auto-follows replies: this proves native node/action context, not a connected PC click. One-liner executes conditions but no action, therefore no dialog event."}

    def custom_faction(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        changes=[]
        for faction in [8,6,7,2]:
            answer=self.command(op="set_faction",target=actor,faction=faction)
            self.until(lambda:self.creature(actor)["faction"]==faction)
            assert answer["faction"]==faction
            changes.append({"requested":faction,"result":answer,"report":dict(self.creature(actor))})
        self.command(expect="invalid",op="set_faction",target=actor,faction=99)
        seq=self.state["seq"];self.until(lambda:self.state["seq"]>seq)
        assert self.creature(actor)["faction"]==2
        return {"custom_name":"test","custom_id":8,"changes":changes,"nonexistent_refused_without_membership_change":True}

    def session_factions(self):
        actors=[self.command(op="spawn",kind="creature",r="nw_bartender",x=x,y=3,z=0,f=0)["obj"] for x in [3,5,7]]
        for actor,faction in zip(actors,[6,7,6]):
            self.command(op="set_faction",target=actor,faction=faction)
            self.command(op="freeze",target=actor,seconds=120)
        self.until(lambda:all(self.creature(a)["faction"]==f for a,f in zip(actors,[6,7,6])))
        prefix='object a=StringToObject("'+actors[0]+'");object b=StringToObject("'+actors[1]+'");object c=StringToObject("'+actors[2]+'");'
        read='json v=JsonArray();v=JsonArrayInsert(v,JsonInt(GetReputation(a,b)));v=JsonArrayInsert(v,JsonInt(GetReputation(c,b)));v=JsonArrayInsert(v,JsonInt(GetReputation(b,a)));MgReturn(JsonDump(v));'
        before=self.command(op="run_chunk",code=prefix+read,**{"return":True})["value"]
        after=self.command(op="run_chunk",code=prefix+'AdjustReputation(b,a,-40);'+read,**{"return":True})["value"]
        assert before==[50,50,50] and after==[10,10,50],(before,after)
        current=self.command(op="get_faction_reputation",target=actors[1],faction=6)
        assert current["value"]["reputation"]==10,current
        changes=[]
        for target,faction,value in [(actors[1],6,75),(actors[1],6,30),(actors[0],7,20),(actors[1],6,80)]:
            changed=self.command(op="set_faction_reputation",target=target,faction=faction,reputation=value)
            assert changed["value"]["reputation"]==value,changed
            changes.append(changed)
        readback=self.command(op="run_chunk",code=prefix+read,**{"return":True})["value"]
        assert readback==[80,80,20],readback
        self.command(op="get_faction_reputation",target=actors[1],faction=99,expect="not_found")
        self.command(op="set_faction_reputation",target=actors[1],faction=6,reputation=101,expect="invalid")
        return {"actors":actors,"before":before,"native_after":after,"operations":changes,"readback":readback,"player_permissions":False,"native_global_direction":"Session 1 regards Session 2 = 80; reverse 20; both Session 1 members match"}

    def vfx_transform(self):
        retired=self.native('JsonString(ResManGetAliasFor("mgcl_aura",RESTYPE_MDL))')
        assert retired=="", "retired HAK must not contribute resources"
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        fields=dict(target=actor,row=2,duration="permanent",scale=1.5,offset=dict(x=1.25,y=-2.5,z=3.75),rotation=dict(x=10,y=20,z=30))
        answer=self.command(op="apply_vfx",**fields)
        self.until(lambda:2 in self.creature(actor).get("vfx",[]))
        code='object a=StringToObject("'+actor+'"); effect e=GetFirstEffect(a); while(GetIsEffectValid(e) && GetEffectTag(e)!="MGLIVE_VFX") e=GetNextEffect(a); json result=JsonObject(); json floats=JsonArray(); int i; for(i=0;i<4;i++) floats=JsonArrayInsert(floats,JsonFloat(GetEffectFloat(e,i))); result=JsonObjectSet(result,"floats",floats); json vectors=JsonArray(); for(i=0;i<2;i++) { vector v=GetEffectVector(e,i); json xyz=JsonArray();xyz=JsonArrayInsert(xyz,JsonFloat(v.x));xyz=JsonArrayInsert(xyz,JsonFloat(v.y));xyz=JsonArrayInsert(xyz,JsonFloat(v.z));vectors=JsonArrayInsert(vectors,xyz); } result=JsonObjectSet(result,"vectors",vectors);MgReturn(JsonDump(result));'
        values=self.command(op="run_chunk",code=code,**{"return":True})["value"]
        assert values["floats"][0]==1.5 and values["vectors"]==[[1.25,-2.5,3.75],[10.,20.,30.]],values
        for bad in [dict(scale=0),dict(scale=101),dict(offset=dict(x=1001,y=0,z=0)),dict(rotation=dict(x=0,y=0,z=3601))]:
            self.command(expect="invalid",op="apply_vfx",**(fields|bad))
        report=dict(self.creature(actor));self.command(op="clear_vfx",target=actor)
        self.until(lambda:2 not in self.creature(actor).get("vfx",[]))
        return {"retired_model_absent":retired=="","result":answer,"native_effect":values,"report":report,"limit":"Native stored parameters; visual Euler interpretation needs client rendering."}

    def inventory_actions(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        other=self.command(op="spawn",kind="creature",r="nw_bartender",x=7,y=3,z=0,f=0)["obj"]
        potion=self.command(op="give_item",target=actor,resref="nw_it_mpotion001")["obj"]
        ring=self.command(op="give_item",target=actor,resref="nw_it_mring001")["obj"]
        read=self.command(op="view_inventory",target=actor)["value"]
        p=next(r for r in read["items"] if r["id"]==potion)
        r=next(r for r in read["items"] if r["id"]==ring)
        assert p["equip_slots"]==0 and r["equip_slots"]==384,(p,r)
        assert p["uses"],p
        for item in [potion,ring]:self.command(expect="invalid",op="equip_item",target=actor,item=item,slot=0)
        self.command(op="equip_item",target=actor,item=ring,slot=8)
        self.command(expect="invalid",op="use_item",target=actor,item=ring)
        self.command(expect="invalid",op="use_item",target=actor,item=potion,property=255)
        # Let native use, not test code, consume the stack and heal the wounded holder.
        self.chunk(f'object a=StringToObject("{actor}"); SetItemStackSize(StringToObject("{potion}"),2); SetImmortal(a,TRUE); ApplyEffectToObject(DURATION_TYPE_INSTANT,EffectDamage(5),a);')
        self.until(lambda:self.creature(actor) and self.creature(actor)["hp"][0]<self.creature(actor)["hp"][1])
        before=dict(self.creature(actor))
        used=self.command(op="use_item",target=actor,item=potion,property=p["uses"][0]["property"])
        self.until(lambda:self.creature(actor)["hp"][0]>before["hp"][0],timeout=20)
        after=dict(self.creature(actor))
        inv=self.command(op="view_inventory",target=actor)["value"]
        assert next(r for r in inv["items"] if r["id"]==potion)["stack"]==1,inv
        self.chunk(f'SetLocalInt(StringToObject("{ring}"),"MGET_TRANSFER",81);')
        moved=self.command(op="transfer_item",target=actor,item=ring,recipient=other)
        new=moved["obj"]
        oldinv=self.command(op="view_inventory",target=actor)["value"]
        newinv=self.command(op="view_inventory",target=other)["value"]
        assert all(r["id"]!=ring for r in oldinv["items"])
        assert any(r["id"]==new for r in newinv["items"])
        assert self.native(f'JsonInt(GetLocalInt(StringToObject("{new}"),"MGET_TRANSFER"))')==81
        self.chunk(f'SetPlotFlag(StringToObject("{new}"),TRUE);')
        self.command(expect="protected",op="transfer_item",target=other,item=new,recipient=actor)
        return {"before_inventory":read,"use_result":used,"before_health":before,"after_health":after,"after_use":inv,"transfer":moved,"recipient_inventory":newinv,"local_preserved":True,"limit":"Native NPC proof. Connected PC charge/UI behaviour still needs owner client."}

    def equipment_look(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bandit001",x=3,y=3,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor).get("look") is not None)
        initial=dict(self.creature(actor))
        inventory=self.command(op="view_inventory",target=actor)["value"]["items"]
        for row in inventory:
            if row["slot"] in [0,1,4,5,6]:
                self.command(op="unequip_item",target=actor,item=row["id"],slot=row["slot"])
        self.until(lambda:self.creature(actor)["look"]["Equip_ItemList"]["value"] == [])
        before=dict(self.creature(actor))
        item=self.command(op="give_item",target=actor,resref="nw_wswls001")["obj"]
        self.command(op="equip_item",target=actor,item=item,slot=4)
        self.until(lambda:self.creature(actor).get("lookHash") != before["lookHash"])
        equipped=dict(self.creature(actor))
        assert any(r["__struct_id"]==16 for r in equipped["look"]["Equip_ItemList"]["value"]),equipped
        self.command(op="unequip_item",target=actor,item=item,slot=4)
        self.until(lambda:self.creature(actor).get("lookHash") == before["lookHash"])
        after=dict(self.creature(actor))
        assert not any(r["__struct_id"]==16 for r in after["look"]["Equip_ItemList"]["value"])
        # Native equipment changes outside Moonglow are detected too.
        self.chunk('AssignCommand(StringToObject("'+actor+'"),ActionEquipItem(StringToObject("'+item+'"),INVENTORY_SLOT_RIGHTHAND));')
        self.until(lambda:self.creature(actor).get("lookHash") == equipped["lookHash"])
        self.chunk('AssignCommand(StringToObject("'+actor+'"),ActionUnequipItem(StringToObject("'+item+'")));')
        self.until(lambda:self.creature(actor).get("lookHash") == before["lookHash"])
        return {"original_equipment":initial,"before":before,"equipped":equipped,"unequipped":after,"external_changes_detected":True,"look_bytes":len(json.dumps(after["look"]))}

    def inventory_grid(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        item=self.command(op="give_item",target=actor,resref="nw_it_mring001")["obj"]
        other=self.command(op="spawn",kind="creature",r="nw_bartender",x=7,y=3,z=0,f=0)["obj"]
        before=self.command(op="view_inventory",target=actor)["value"]
        ring=next(r for r in before["items"] if r["id"]==item)
        assert ring["slot"]==-1 and "BaseItem" in ring["look"],ring
        self.command(expect="invalid",op="equip_item",target=actor,item=item,slot=14)
        self.command(expect="not_found",op="equip_item",target=other,item=item,slot=7)
        equipped=self.command(op="equip_item",target=actor,item=item,slot=7)
        assert equipped["value"]["slot"]==7,equipped
        read=self.command(op="view_inventory",target=actor)["value"]
        assert next(r for r in read["items"] if r["id"]==item)["slot"]==7,read
        assert self.native('JsonString(ObjectToString(GetItemInSlot(7,StringToObject("'+actor+'"))))')==item
        self.command(expect="invalid",op="unequip_item",target=actor,item=item,slot=8)
        unequipped=self.command(op="unequip_item",target=actor,item=item,slot=7)
        assert unequipped["value"]["slot"]==-1,unequipped
        read2=self.command(op="view_inventory",target=actor)["value"]
        assert next(r for r in read2["items"] if r["id"]==item)["slot"]==-1,read2
        self.command(op="run_chunk",code='SetPlotFlag(StringToObject("'+item+'"),TRUE);')
        self.command(expect="protected",op="drop_item",target=actor,item=item)
        self.command(op="run_chunk",code='SetPlotFlag(StringToObject("'+item+'"),FALSE);')
        item=self.command(op="drop_item",target=actor,item=item)["obj"]
        self.until(lambda:self.object(item) is not None)
        assert self.native('JsonBool(GetIsObjectValid(GetItemPossessor(StringToObject("'+item+'"))))') is False
        return {"initial_inventory":before,"equipped":read,"unequipped":read2,"native_unequip_result":unequipped,"ground_report":self.object(item),"note":"NPC native equip/drop effect; connected PC needs owner check; backpack cell arrangement has no exposed native setter"}

    def creature_animation(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        before=self.creature(actor)
        for fields in [{"animation":200},{"animation":99},{"animation":4294967296},{"animation":"../bad"},{"animation":3,"speed":0},{"animation":3,"seconds":301},{"animation":3,"loop":1}]:
            self.command(expect="invalid",op="play_creature_animation",target=actor,**fields)
        standard=self.command(op="play_creature_animation",target=actor,animation=3,loop=True,speed=0.5,seconds=4)
        self.until(lambda:self.creature(actor).get("animation",{}).get("animation")==3 and self.creature(actor)["anim"]!=before["anim"])
        during=self.creature(actor)
        self.until(lambda:"animation" not in self.creature(actor),timeout=10)
        # Temporary named-slot use preserves earlier native replacement storage.
        old=self.command(op="replace_object_animation",target=actor,old="custom70",new="pause2")["value"]["state"]
        custom=self.command(op="play_creature_animation",target=actor,animation="talknormal",loop=True,speed=1.0,seconds=4)
        self.until(lambda:self.creature(actor).get("animation",{}).get("animation")=="talknormal")
        changed=self.native('JsonObjectGet(ObjectToJson(StringToObject("'+actor+'")),"AnimationReplace")')
        assert 'talknormal' in json.dumps(changed),changed
        self.until(lambda:"animation" not in self.creature(actor),timeout=10)
        restored=self.native('JsonObjectGet(ObjectToJson(StringToObject("'+actor+'")),"AnimationReplace")')
        assert restored==old,(old,restored)
        oneshot=self.command(op="play_creature_animation",target=actor,animation=105,loop=True,speed=2.0,seconds=6)
        self.until(lambda:self.creature(actor).get("animation",{}).get("cycles",0)>=2)
        repeated=self.creature(actor)
        self.until(lambda:"animation" not in self.creature(actor),timeout=10)
        self.command(op="play_creature_animation",target=actor,animation="pause2",loop=False,speed=1,seconds=20)
        self.command(op="npc_move",target=actor,x=5,y=3,z=0,run=False,hold=True,timeout=5)
        self.until(lambda:"animation" not in self.creature(actor) and self.creature(actor)["x"]>4.0)
        self.command(op="play_creature_animation",target=actor,animation="talknormal",loop=True,seconds=2)
        self.command(op="play_creature_animation",target=actor,animation="pause2",loop=True,seconds=6)
        started=time.monotonic()
        self.until(lambda:time.monotonic()-started>=2.2)
        assert self.creature(actor).get("animation",{}).get("animation")=="pause2",self.creature(actor)
        self.command(op="replace_object_animation",target=actor,old="custom70",new="walk")
        self.until(lambda:"animation" not in self.creature(actor),timeout=10)
        external=self.native('JsonObjectGet(ObjectToJson(StringToObject("'+actor+'")),"AnimationReplace")')
        assert 'walk' in json.dumps(external),external
        return {"before":before,"standard_result":standard,"standard_report":during,"custom_result":custom,"native_custom_mapping":changed,"restored_mapping":restored,"external_override_preserved":external,"old_timer_preserves_new_order":True,"repeat_one_shot":oneshot,"repeated_report":repeated,"after_move":self.creature(actor),"client_limit":"native pose/request and replacement storage proven; custom model rendering needs connected client"}

    def portrait(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        before=self.native('JsonString(GetPortraitResRef(OBJECT_SELF))',actor)
        self.until(lambda:self.creature(actor).get("portrait")==before)
        self.command(op="run_chunk",code='SetPortraitResRef(OBJECT_SELF,"custom_test");',target=actor)
        actual=self.native('JsonString(GetPortraitResRef(OBJECT_SELF))',actor)
        assert actual=="custom_test",actual
        self.until(lambda:self.creature(actor).get("portrait")==actual)
        return {"native_original":before,"custom_native":actual,"report":self.creature(actor),"note":"Engine base is unchanged; no prefix or size letter added by transport."}

    def economy(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bandit001",x=3,y=3,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        self.command(op="run_chunk",code='GiveGoldToCreature(OBJECT_SELF,100);',target=actor)
        before=self.native('JsonInt(GetGold(OBJECT_SELF))',actor)
        take=self.command(op="take_gold",target=actor,amount=25)["value"]
        assert take["before"]==before and take["after"]==before-25,take
        self.until(lambda:self.creature(actor).get("gold")==before-25)
        emptied=self.command(op="take_gold",target=actor,amount=1000000)["value"]
        assert emptied["before"]==before-25 and emptied["after"]==0,emptied
        self.until(lambda:self.creature(actor).get("gold")==0)
        invalid=[{"op":"take_gold","amount":0},{"op":"take_gold","amount":1000001},{"op":"take_xp","amount":100001},{"op":"take_xp","amount":0},{"op":"set_xp","value":-1},{"op":"set_xp","value":4294967296}]
        for fields in invalid:self.command(expect="invalid",target=actor,**fields)
        self.command(op="run_script",target=actor,script="mget_levels")
        level=self.native('JsonInt(GetHitDice(OBJECT_SELF))',actor)
        # Native NPC fixture has multiple actual levels; XP alone does not add them.
        assert level>=2,level
        original=self.command(op="set_xp",target=actor,value=10000)["value"]
        self.until(lambda:self.creature(actor).get("xp")==10000)
        taken=self.command(op="take_xp",target=actor,amount=1)["value"]
        assert taken["after"]==9999,taken
        self.until(lambda:self.creature(actor).get("xp")==9999)
        stats_before=self.native('JsonObjectSet(JsonObjectSet(JsonObject(),"max_hp",JsonInt(GetMaxHitPoints(OBJECT_SELF))),"bab",JsonInt(GetBaseAttackBonus(OBJECT_SELF)))',actor)
        lost=self.command(op="remove_level",target=actor)["value"]
        threshold=self.native('JsonInt(StringToInt(Get2DAString("exptable","XP",'+str(level-2)+')))')
        assert lost["after"]==threshold,lost
        self.until(lambda:self.creature(actor).get("xp")==threshold)
        actual=self.native('JsonInt(GetHitDice(OBJECT_SELF))',actor)
        stats_after=self.native('JsonObjectSet(JsonObjectSet(JsonObject(),"max_hp",JsonInt(GetMaxHitPoints(OBJECT_SELF))),"bab",JsonInt(GetBaseAttackBonus(OBJECT_SELF)))',actor)
        final=self.command(op="take_xp",target=actor,amount=100000)["value"]
        assert final["after"]==0,final
        self.until(lambda:self.creature(actor).get("xp")==0)
        self.command(expect="invalid",op="remove_level",target=actor)
        return {"stats_before":stats_before,"stats_after":stats_after,"gold":take,"gold_zero":emptied,"set_xp":original,"take_xp":taken,"remove_level":lost,"level_native_after_remove":actual,"zero":final,"report":self.creature(actor),"pc":"requires connected client; gates covered by core tests"}

    def pc_skill_rolls(self):
        baseline=self.pc_skill_nui()
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        def stage(n):
            self.chunk(f'SetLocalInt(GetModule(),"MGET_ROLL_STAGE",{n});')
            self.command(op="run_script",target=actor,script="mget_roll")
        snapshots=[]
        for n,key in enumerate(["BASE","FEAT","ITEM","EFFECT"]):
            stage(n)
            value=self.native('GetLocalJson(GetModule(),"MGET_SKILL_'+key+'")')
            if value is None:
                self.until(lambda:self.state["seq"]>0)
                value=self.native('GetLocalJson(GetModule(),"MGET_SKILL_'+key+'")')
            snapshots.append(value)
        assert [v["base"] for v in snapshots]==[5]*4,snapshots
        assert [v["total"] for v in snapshots]==[9,12,16,21],snapshots
        stage(4)
        breakdown=self.native('JsonString(GetLocalString(GetModule(),"MGET_SKILL_BREAKDOWN"))')
        assert breakdown=="Ranks 5  |  CON +4  |  Other +12  =  +21",breakdown
        pending=self.native('GetLocalJson(GetModule(),"MGET_PROMPT")')
        assert pending["pending"] and pending["value"]["players"][0]["status"]=="waiting",pending
        assert self.native('GetLocalJson(GetModule(),"MGET_BUSY")')["status"]=="busy"
        assert not self.matching("skill_roll"),"Sending a prompt rolled without a click"
        stage(5)
        self.until(lambda:any(e.get("request")=="MGET_ROLL" for e in self.matching("skill_roll")))
        self.until(lambda:any(r["id"]=="MGET_ROLL" for f in self.frames for r in f["results"]))
        rolled=next(r for f in self.frames for r in f["results"] if r["id"]=="MGET_ROLL")
        row=rolled["value"]["players"][0]
        assert not rolled["pending"] and row["status"]=="rolled" and 1<=row["roll"]<=20,row
        assert row["modifier"]==21 and row["total"]==row["roll"]+21+2 and row["passed"],row
        event=next(e for e in self.matching("skill_roll") if e.get("request")=="MGET_ROLL")
        assert f'{row["roll"]} + 21 + 2 (DM) = {row["total"]} vs DC 15: success' in event["text"],event
        stage(6)
        self.until(lambda:any(r["id"]=="MGET_NO_ROLL" for f in self.frames for r in f["results"]),10)
        timeout=next(r for f in self.frames for r in f["results"] if r["id"]=="MGET_NO_ROLL")
        assert not timeout["pending"] and timeout["value"]["players"][0]["status"]=="no_roll",timeout
        stage(7);stage(5)
        self.until(lambda:any(r["id"]=="MGET_FAIL" for f in self.frames for r in f["results"]))
        failed=next(r for f in self.frames for r in f["results"] if r["id"]=="MGET_FAIL")
        assert not failed["pending"] and not failed["value"]["players"][0]["passed"],failed
        failure_event=next(e for e in self.matching("skill_roll") if e.get("request")=="MGET_FAIL")
        assert " vs DC " not in failure_event["text"] and failure_event["text"].endswith(": failure"),failure_event
        assert sum(e.get("request")=="MGET_ROLL" for e in self.matching("skill_roll"))==1
        assert self.native('JsonInt(GetLocalInt(GetModule(),"MGLIVE_SKILL_ACTIVE"))')==0
        assert self.native('JsonInt(GetLocalInt(OBJECT_SELF,"MGLIVE_SKILL_GEN"))',actor)==3
        for extra in [{"bonus":101},{"bonus":-101},{"bonus":True},{"timeout":0},{"timeout":601},{"show_dc":1}]:
            self.command(expect="invalid",op="skill_check",players=True,x=3,y=3,radius=10,skill=1,dc=15,**extra)
        return {"nui":baseline,"rank_base_ability_feat_item_effect":snapshots,"bonus_breakdown":breakdown,"pending":pending,"rolled":rolled,"event":event,"timeout":timeout,"failure":failed,"hidden_dc_event":failure_event,"duplicate_click_ignored":True,
                "limitation":"No connected PC: exact roll/helper and timeout proven on an NPC stand-in, NUI JSON and no-client NuiCreate proven. Real Roll/OK click dispatch, positive player token, client UI and SendMessageToPC output need owner client."}

    def pc_skill_nui(self):
        baseline = self.pc_skill_check()
        self.command(op="run_script", script="mget_nui")
        proof = self.native('GetLocalJson(GetModule(), "MGET_NUI_PROOF")')
        assert proof["handler"] == "mglive_nuievt" and proof["previous"] == "mget_nuiold", proof
        assert proof["forwarded"] == 1 and proof["no_client_token"] == 0, proof
        def values(node):
            if isinstance(node, dict):
                for v in node.values(): yield from values(v)
            elif isinstance(node, list):
                for v in node: yield from values(v)
            else: yield node
        success=list(values(proof["window"])); failure=list(values(proof["failure"]))
        assert all(v in success for v in ["Concentration check", "DC 15", "+2 DM bonus", "Roll", "roll", "OK", "ok", 324, 52, "finished", "outcome_color", "has_message"]), success
        assert "DC 25" not in failure and "-2 DM bonus" in failure and "DC hidden" in failure, failure
        zero=list(values(proof["zero_bonus"]))
        assert not any("DM bonus" in v for v in zero if isinstance(v,str)),zero
        assert "locked" not in json.dumps(proof) and "textedit" not in json.dumps(proof).lower()
        assert "Click Roll when you are ready" not in json.dumps(proof)
        return {"baseline":baseline, "nui":proof,
                "limitation":"No connected PC: native NuiCreate returned zero as expected; positive token, client rendering, OK and real timeout await owner's client."}

    def pc_skill_check(self):
        self.command(expect="disabled",op="skill_check",x=3,y=5,radius=30,skill=0,dc=15)
        empty=self.command(op="skill_check",players=True,x=3,y=5,radius=30,skill=0,dc=15,success_text="",fail_text="")
        assert empty["value"]["players"]==[] and not empty["value"]["truncated"] and not empty["pending"],empty
        for fields in [dict(radius=31),dict(dc=256),dict(skill=4294967296),dict(success_text="x"*201)]:
            args=dict(x=3,y=5,radius=30,skill=0,dc=15);args.update(fields)
            self.command(expect="invalid",op="skill_check",players=True,**args)
        self.command(expect="not_found",op="skill_check",players=True,x=3,y=5,radius=30,skill=65535,dc=15)
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        native=self.native('JsonArrayInsert(JsonArrayInsert(JsonArray(),JsonBool(GetIsSkillSuccessful(OBJECT_SELF, SKILL_CONCENTRATION, 0))),JsonBool(GetIsSkillSuccessful(OBJECT_SELF, SKILL_CONCENTRATION, 255)))',actor)
        assert native==[True,False],native
        return {"empty_pc_result":empty,"native_npc_skill_extremes":native,"limitation":"No connected private client: PC filtering and messages to a real PC remain unverified; native skill rolls proven on NPC only."}

    def capture(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(op="set_name",target=actor,name="Library keeper")
        self.command(op="set_appearance",target=actor,row=0)
        self.command(op="run_chunk",target=actor,code='CreateItemOnObject("nw_it_mpotion001", OBJECT_SELF, 3); string d="x"; while(GetStringLength(d)<18000) d+=d; SetDescription(OBJECT_SELF,GetStringLeft(d,18000));')
        inventory=self.command(op="view_inventory",target=actor)["value"]
        capture=self.command(op="capture_object",target=actor)["object_json"]
        assert capture["__data_type"]=="UTC "
        assert len(json.dumps(capture,separators=(",",":"))) > 16384
        self.command(op="destroy",target=actor)
        self.until(lambda:self.creature(actor) is None)
        clone=self.command(op="spawn_json",json=capture,x=6,y=5,z=0,f=90)["obj"]
        self.until(lambda:self.creature(clone) is not None)
        after=self.creature(clone)
        assert clone!=actor and after["app"]==0 and after["name"]=="Library keeper",after
        assert abs(after["x"]-6)<.2 and abs(after["y"]-5)<.2,after
        cloned_inventory=self.command(op="view_inventory",target=clone)["value"]
        def comparable(rows):return sorted((i["resref"],i["stack"],i["slot"]) for i in rows["items"])
        assert set(comparable(inventory)) <= set(comparable(cloned_inventory)),(inventory,cloned_inventory)
        assert self.native("JsonInt(GetStringLength(GetDescription(OBJECT_SELF)))",clone)==18000
        self.command(op="run_chunk",target=clone,code='string d="x"; while(GetStringLength(d)<262144) d+=d; object item=GetFirstItemInInventory(OBJECT_SELF); while(GetIsObjectValid(item)){SetDescription(item,d); item=GetNextItemInInventory(OBJECT_SELF);}')
        oversized_bytes=self.native("JsonInt(GetStringLength(JsonDump(ObjectToJson(OBJECT_SELF))))",clone)
        assert oversized_bytes>262144,oversized_bytes
        oversized_capture=self.command(expect="invalid",op="capture_object",target=clone)
        invalids=[None,{}, {"__data_type":"BAD "}, {**capture,"IsPC":{"type":"byte","value":1}}, {**capture,"Bad":{"type":"byte","value":256}}, {**capture,"Tag":{"type":"string","value":"x"*262144}}]
        for bad in invalids:self.command(expect="invalid",op="spawn_json",json=bad,x=6,y=5,z=0,f=0)
        return {"source":actor,"clone":after,"capture_bytes":len(json.dumps(capture,separators=(",",":"))),"inventory_before":inventory,"inventory_after":cloned_inventory,"on_spawn_may_add_items":True,"oversized_bytes":oversized_bytes,"oversized_capture":oversized_capture,"invalid_cases":len(invalids),"limitation":"Same-boot serialize/destroy/restore proves independence from old object ID, not week-long persistence or cross-module resource availability."}

    def capture_probe(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        return self.command(op="run_chunk",target=actor,code='json j=ObjectToJson(OBJECT_SELF, FALSE); json p=JsonObject(); p=JsonObjectSet(p,"bytes",JsonInt(GetStringLength(JsonDump(j)))); p=JsonObjectSet(p,"type",JsonObjectGet(j,"__data_type")); p=JsonObjectSet(p,"app",JsonObjectGet(j,"Appearance_Type")); p=JsonObjectSet(p,"pc",JsonObjectGet(j,"IsPC")); p=JsonObjectSet(p,"script",JsonObjectGet(j,"ScriptSpawn")); p=JsonObjectSet(p,"inventory",JsonObjectGet(j,"ItemList")); MgReturn(JsonDump(p));',**{"return":True})

    def appearance(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        before=self.creature(actor)
        assert before["app"]!=0
        result=self.command(op="set_appearance",target=actor,row=0)
        self.until(lambda:self.creature(actor)["app"]==0)
        native=self.native("JsonInt(GetAppearanceType(OBJECT_SELF))",actor)
        assert native==0 and result["value"]["app"]==0
        after=self.creature(actor)
        self.command(expect="invalid",op="set_appearance",target=actor,row=4294967296)
        self.command(expect="not_found",op="set_appearance",target=actor,row=65535)
        keys=self.native("JsonObjectKeys(ObjectToJson(OBJECT_SELF))",actor)
        return {"before":before,"after":after,"native":native,"result":result,"capture_keys":keys,"limitation":"Native appearance field, no model render proof."}

    def apply_vfx(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(op="run_chunk",target=actor,code='ApplyEffectToObject(DURATION_TYPE_PERMANENT, TagEffect(EffectVisualEffect(411), "OTHER_SCRIPT"), OBJECT_SELF);')
        self.command(op="apply_vfx",target=actor,row=411,duration="permanent")
        self.until(lambda:self.creature(actor)["vfx"].count(411)==2)
        two=self.creature(actor)
        removed=self.command(op="clear_vfx",target=actor,row=411)
        assert removed["value"]["removed"]==1,removed
        self.until(lambda:self.creature(actor)["vfx"]==[411])
        foreign=self.native('JsonBool(GetEffectTag(GetFirstEffect(OBJECT_SELF))=="OTHER_SCRIPT")',actor)
        assert foreign
        self.command(op="apply_vfx",target=actor,row=412,duration="temporary",seconds=3)
        self.until(lambda:412 in self.creature(actor)["vfx"])
        temporary=self.creature(actor)
        self.until(lambda:self.creature(actor)["vfx"]==[411],12)
        self.command(op="apply_vfx",target=actor,row=412,duration="permanent")
        self.until(lambda:412 in self.creature(actor)["vfx"])
        all_ours=self.command(op="clear_vfx",target=actor)
        assert all_ours["value"]["removed"]==1
        self.until(lambda:self.creature(actor)["vfx"]==[411])
        instant=self.command(op="apply_vfx",target=actor,row=9,duration="instant")
        location=self.command(op="apply_vfx",location={"x":5,"y":5,"z":0},row=9,duration="instant")
        for fields in [dict(row=-1,duration="instant"),dict(row=411,duration="temporary",seconds=0),dict(row=411,duration="permanent",seconds=2),dict(row=411,duration="bad"),dict(row=4294967707,duration="instant")]:
            self.command(expect="invalid",op="apply_vfx",target=actor,**fields)
        self.command(expect="invalid",op="apply_vfx",target=actor,location={"x":1,"y":1,"z":0},row=411,duration="permanent")
        self.command(expect="not_found",op="apply_vfx",target=actor,row=65535,duration="instant")
        return {"two_same_rows":two,"only_ours_removed":removed,"foreign_survived":foreign,"temporary":temporary,"clear_all":all_ours,"instant_submitted":instant,"location_submitted":location,"limitation":"Lasting effects independently verified in report and native tag readback; instant/location render requires a client."}

    def kill_repro(self):
        return self.kill_override(reproduce=True)

    def kill_override(self, reproduce=False):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(op="run_chunk",target=actor,code='SetIsDestroyable(FALSE); SetEventScript(OBJECT_SELF, EVENT_SCRIPT_CREATURE_ON_DEATH, ""); SetPlotFlag(OBJECT_SELF, TRUE); SetImmortal(OBJECT_SELF, TRUE);')
        before=self.native('JsonParse("["+IntToString(GetPlotFlag(OBJECT_SELF))+","+IntToString(GetImmortal(OBJECT_SELF))+"]")',actor)
        assert before==[1,1],before
        answer=self.command(expect="protected" if reproduce else "ok",op="kill",target=actor)
        if reproduce:
            assert not self.creature(actor)["dead"]
        else:
            self.until(lambda:self.creature(actor) is not None and self.creature(actor)["dead"])
            assert answer["value"]=={"dead":True,"overridden":{"plot":True,"immortal":True}},answer
            assert self.native(f'JsonBool(GetPlotFlag(StringToObject("{actor}")) && GetImmortal(StringToObject("{actor}")))')
            assert self.native('JsonBool(GetIsObjectValid(StringToObject("'+actor+'")))')
            self.command(expect="dead",op="kill",target=actor)
        return {"before_flags":before,"result":answer,"after":self.creature(actor),"reproduced":reproduce}

    def hostile_repro(self):
        return self.hostile_fight(reproduce=True)

    def hostile_fight(self, reproduce=False):
        a=self.command(op="spawn",kind="creature",r="nw_bandit001",x=3,y=5,z=0,f=0,faction=1)["obj"]
        b=self.command(op="spawn",kind="creature",r="nw_bartender",x=6,y=5,z=0,f=180,faction=1)["obj"]
        self.command(op="set_immortal",target=b,enabled=True)
        start=self.state["seq"]
        answer=self.command(op="npc_attack",target=a,victim=b,hold=True,timeout=12)
        if reproduce:
            self.until(lambda:self.state["seq"]>=start+9)
            assert not any(e.get("id")==a for e in self.matching("attack"))
            assert self.creature(a)["faction"]==1 and not self.creature(a)["combat"]
            return {"reproduced":True,"answer":answer,"actor":self.creature(a),"victim":self.creature(b)}
        self.until(lambda:any(e.get("id")==a and e.get("target")==b for e in self.matching("attack")),30)
        assert self.creature(a)["faction"]!=1
        during=self.creature(a)
        self.until(lambda:self.creature(a)["faction"]==1,20)
        # A named spare faction is also accepted, restored on a new move order.
        explicit=self.command(op="npc_attack",target=a,victim=b,faction=5,hold=True,timeout=20)
        self.until(lambda:self.creature(a)["faction"]==5 and self.creature(a)["combat"],15)
        self.command(op="npc_move",target=a,x=3,y=3,z=0,hold=True)
        self.until(lambda:self.creature(a)["faction"]==1)
        self.command(expect="invalid",op="npc_attack",target=a,victim=b,faction=1)
        self.command(expect="not_found",op="npc_attack",target=a,victim=b,faction=65535)
        return {"default":answer,"during":during,"explicit":explicit,"restored":self.creature(a),"attack":next(e for e in self.matching("attack") if e.get("id")==a)}

    def pc_dm_bounds(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        chest=self.furniture()[0]
        wide=4294967296
        cases=[{"op":"heal","target":actor,"amount":wide+1},
               {"op":"set_time","hour":wide+1,"minute":0},
               {"op":"set_date","year":wide+1400,"month":1,"day":1},
               {"op":"spawn_trap_on_object","target":chest,"trap":wide},
               {"op":"set_faction","target":actor,"faction":wide+2},
               {"op":"give_alignment","target":actor,"law_chaos":wide+1,"good_evil":0},
               {"op":"set_stat","target":actor,"ability":"strength","value":wide+19},
               {"op":"set_faction_reputation","target":actor,"faction":3,"reputation":wide+75},
               {"op":"set_variable","target":"module","type":"int","name":"wide","value":wide+1}]
        for case in cases:self.command(expect="invalid",players=True,**case)
        return {"wide_integers_refused":cases}

    def dm_player_commands(self, target):
        return [{"op":"give_level","target":target},{"op":"give_alignment","target":target,"law_chaos":10,"good_evil":-20},{"op":"set_stat","target":target,"ability":"strength","value":19},{"op":"get_faction_reputation","target":target,"faction":"merchant"},{"op":"set_faction_reputation","target":target,"faction":"merchant","reputation":75}]

    def dm_players_disabled(self):
        for command in self.dm_player_commands("7ffffffe"):
            self.command(expect="disabled",players=True,**command)
        return {"all_five_disabled_before_target_resolution":True}

    def pc_dm_players(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        for command in self.dm_player_commands(actor):self.command(expect="disabled",**command)
        level=self.native('JsonInt(GetHitDice(OBJECT_SELF))',actor)
        xp=self.command(op="give_level",target=actor,players=True)["value"]
        threshold=self.native('JsonInt(StringToInt(Get2DAString("exptable","XP",'+str(level)+')))')
        assert xp["xp"]==threshold and xp["eligible_level"]==level+1,xp
        self.until(lambda:self.creature(actor).get("xp")==threshold)
        self.command(expect="invalid",op="give_level",target=actor,players=True)
        before=[self.native('JsonInt(GetLawChaosValue(OBJECT_SELF))',actor),self.native('JsonInt(GetGoodEvilValue(OBJECT_SELF))',actor)]
        alignment=self.command(op="give_alignment",target=actor,law_chaos=10,good_evil=-20,players=True)["value"]
        assert alignment=={"law_chaos":min(100,before[0]+10),"good_evil":max(0,before[1]-20)},alignment
        assert self.native('JsonInt(GetLawChaosValue(OBJECT_SELF))',actor)==alignment["law_chaos"]
        assert self.native('JsonInt(GetGoodEvilValue(OBJECT_SELF))',actor)==alignment["good_evil"]
        abilities={}
        for i,ability in enumerate(["strength","dexterity","constitution","intelligence","wisdom","charisma"]):
            result=self.command(op="set_stat",target=actor,ability=ability,value=19+i,players=True)["value"]
            # Human fixture: the native base score has no racial adjustment.
            actual=self.native('JsonInt(GetAbilityScore(OBJECT_SELF,'+str(i)+',TRUE))',actor)
            assert result==actual==19+i,(ability,result,actual)
            abilities[ability]=actual
        representative=self.command(op="spawn",kind="creature",r="nw_bartender",x=9,y=9,z=0,f=0,faction=5)["obj"]
        reps=[]
        for faction in ["merchant",5]:
            before=self.command(op="get_faction_reputation",target=actor,faction=faction,players=True)["value"]
            changed=self.command(op="set_faction_reputation",target=actor,faction=faction,reputation=75,players=True)["value"]
            read=self.command(op="get_faction_reputation",target=actor,faction=faction,players=True)["value"]
            assert changed["reputation"]==read["reputation"]==75,(before,changed,read)
            expr='JsonInt(GetStandardFactionReputation(STANDARD_FACTION_MERCHANT,StringToObject("'+actor+'")))' if faction=="merchant" else 'JsonInt(GetFactionAverageReputation(StringToObject("'+representative+'"),StringToObject("'+actor+'")))'
            assert self.native(expr)==75
            reps.append({"before":before,"changed":changed,"read":read})
        for command in [{"op":"give_alignment","law_chaos":101,"good_evil":0},{"op":"set_stat","ability":"ac","value":12},{"op":"set_stat","ability":"strength","value":256},{"op":"set_faction_reputation","faction":3,"reputation":101}]:self.command(expect="invalid",target=actor,players=True,**command)
        self.command(expect="not_found",op="get_faction_reputation",target=actor,faction=65535,players=True)
        return {"surrogate":"NPC: no private player client attached","xp":xp,"alignment":alignment,"abilities":abilities,"reputations":reps,"missing_command_flag_refused":True}

    def dm_traps(self):
        observations=[]
        for target in self.furniture():
            self.command(op="spawn_trap_on_object",target=target,trap=0)
            active=self.native('JsonBool(GetIsTrapped(StringToObject("'+target+'")))')
            kind=self.native('JsonInt(GetTrapBaseType(StringToObject("'+target+'")))')
            assert active and kind==0,(active,kind)
            self.command(expect="invalid",op="spawn_trap_on_object",target=target,trap=1)
            self.command(op="disable_trap",target=target)
            inactive=self.native('JsonBool(GetIsTrapped(StringToObject("'+target+'")))')
            assert not inactive
            self.command(op="spawn_trap_on_object",target=target,trap=1)
            assert self.native('JsonInt(GetTrapBaseType(StringToObject("'+target+'")))')==1
            self.command(op="disable_trap",target=target)
            self.command(expect="not_found",op="spawn_trap_on_object",target=target,trap=65535)
            for value in [-1,65536,True,1.5]:self.command(expect="invalid",op="spawn_trap_on_object",target=target,trap=value)
            observations.append({"target":target,"active":active,"first_type":kind,"disabled":not inactive,"replacement_type":1})
        return observations

    def calendar(self):
        code='json j=JsonObject(); j=JsonObjectSet(j,"year",JsonInt(GetCalendarYear())); j=JsonObjectSet(j,"month",JsonInt(GetCalendarMonth())); j=JsonObjectSet(j,"day",JsonInt(GetCalendarDay())); j=JsonObjectSet(j,"hour",JsonInt(GetTimeHour())); j=JsonObjectSet(j,"minute",JsonInt((GetTimeMinute()*60+GetTimeSecond())/2)); j=JsonObjectSet(j,"native_minute",JsonInt(GetTimeMinute())); j=JsonObjectSet(j,"native_second",JsonInt(GetTimeSecond())); MgReturn(JsonDump(j));'
        return self.command(op="run_chunk",code=code,**{"return":True})["value"]

    def dm_world(self):
        self.command(expect="disabled",op="jump_all_players",x=1,y=2,z=0,players=True)
        self.command(op="set_date",year=1372,month=6,day=2)
        date=self.calendar()
        assert [date[k] for k in ["year","month","day"]]==[1372,6,2],date
        self.command(op="set_time",hour=13,minute=17)
        time1=self.calendar()
        assert time1["hour"]==13 and time1["minute"] in [17,18],time1
        self.command(op="set_time",hour=6,minute=30)
        time2=self.calendar()
        assert time2["day"]==time1["day"]+1 and time2["hour"]==6 and time2["minute"] in [30,31],time2
        self.command(expect="invalid",op="set_date",year=1372,month=1,day=1)
        for fields in [{"op":"set_time","hour":24,"minute":0},{"op":"set_time","hour":1,"minute":True},{"op":"set_date","year":32001,"month":1,"day":1},{"op":"set_date","year":1401,"month":1,"day":29}]:self.command(expect="invalid",**fields)
        return {"date":date,"time":time1,"earlier_time_advances_day":time2,"players_disabled":True}

    def pc_dm_world(self):
        self.command(expect="disabled",op="jump_all_players",x=1,y=2,z=0)
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        before=self.creature(actor)
        result=self.command(op="jump_all_players",x=12,y=8,z=0,players=True)
        assert result["affected"]==[],result
        after=self.creature(actor)
        assert [before[k] for k in ["x","y","z"]]==[after[k] for k in ["x","y","z"]]
        return {"no_connected_clients":True,"result":result,"npc_unchanged":True,"needs_client":"actual PC movement not exercised"}

    def dm_inventory(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bandit001",x=3,y=5,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        baseline=self.command(op="view_inventory",target=actor)["value"]
        self.command(op="run_chunk",target=actor,code='object a=CreateItemOnObject("nw_it_mpotion001",OBJECT_SELF,3); SetTag(a,"DM_POTION");')
        # Native NPC spawn scripts equip their authored gear after the first tick.
        self.native("JsonInt(GetTimeSecond())")
        rows=self.command(op="view_inventory",target=actor)["value"]["items"]
        potion=next(i for i in rows if i["tag"]=="DM_POTION")
        assert potion["stack"]==3 and potion["resref"]=="nw_it_mpotion001" and potion["name"] and potion["slot"]==-1,potion
        weapon=next(i for i in rows if i["slot"]>=0)
        assert self.native('JsonString(ObjectToString(GetItemInSlot('+str(weapon["slot"])+',StringToObject("'+actor+'"))))')==weapon["id"]
        for item in rows:
            assert self.native('JsonInt(GetItemStackSize(StringToObject("'+item["id"]+'")))')==item["stack"]
        chest=self.furniture()[0]
        self.command(expect="not_found",op="take_item",target=chest,item=potion["id"])
        self.command(op="run_chunk",target=actor,code='SetPlotFlag(StringToObject("'+potion["id"]+'"),TRUE);')
        self.command(expect="protected",op="take_item",target=actor,item=potion["id"])
        self.command(op="run_chunk",target=actor,code='SetPlotFlag(StringToObject("'+potion["id"]+'"),FALSE);')
        self.command(op="take_item",target=actor,item=potion["id"])
        assert not self.native('JsonBool(GetIsObjectValid(StringToObject("'+potion["id"]+'")))')
        self.command(op="take_item",target=actor,resref=weapon["resref"])
        remaining=self.command(op="view_inventory",target=actor)["value"]
        assert not {i["id"] for i in remaining["items"]}&{potion["id"],weapon["id"]}
        self.command(expect="invalid",op="take_item",target=actor)
        self.command(expect="invalid",op="take_item",target=actor,item=actor,resref="x")
        return {"before":baseline,"items":rows,"after":remaining,"foreign_holder_refused":True,"plot_refused":True}

    def dm_variables(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        observations=[]
        for target in [actor,"area","module"]:
            for kind,value in [("int",-123),("float",1.25),("string","agent local"),("object",actor),("location",{"area":"events","x":12,"y":8,"z":0,"f":90})]:
                name="DM_"+kind
                result=self.command(op="set_variable",target=target,type=kind,name=name,value=value)
                assert result["value"]==value,result
                read=self.command(op="get_variable",target=target,type=kind,name=name)
                assert read["value"]==value,read
                native_target='StringToObject("'+actor+'")' if target==actor else ('GetModule()' if target=="module" else 'GetArea(OBJECT_SELF)')
                if target=="area":native_target='OBJECT_SELF'
                expr={"int":"JsonInt(GetLocalInt(","float":"JsonFloat(GetLocalFloat(","string":"JsonString(GetLocalString(","object":"JsonString(ObjectToString(GetLocalObject("}
                if kind in expr:
                    observed=self.native(expr[kind]+native_target+', "'+name+'"))'+(')' if kind=="object" else ''))
                    assert observed==value,(kind,observed)
                else:
                    assert self.native('JsonFloat(GetFacingFromLocation(GetLocalLocation('+native_target+', "'+name+'")))')==90
                observations.append({"target":target,"type":kind,"value":read["value"]})
            locals=self.command(op="dump_locals",target=target)["value"]
            assert {r["name"] for r in locals["locals"]}>={"DM_int","DM_float","DM_string","DM_object","DM_location"},locals
            assert not any(r["name"].upper().startswith(("MGLIVE_","NWNX_")) for r in locals["locals"])
        self.command(expect="invalid",op="set_variable",target="module",type="int",name="MGLIVE_ALLOW_PLAYER_OPS",value=1)
        for kind,value in [("int",True),("int",2147483648),("float","x"),("string","x"*4097),("location",{"area":"events","x":1})]:
            self.command(expect="invalid",op="set_variable",target=actor,type=kind,name="bad",value=value)
        for kind in ["object","location"]:
            self.command(op="set_variable",target=actor,type=kind,name="DM_"+kind,value=None)
            assert self.command(op="get_variable",target=actor,type=kind,name="DM_"+kind)["value"] is None
        self.chunk('int i; for(i=0;i<160;i++) SetLocalInt(GetModule(), "bound_"+IntToString(i), i+1);')
        bounded=self.command(op="dump_locals",target="module")["value"]
        assert bounded["truncated"] and len(bounded["locals"])<=128 and len(json.dumps(bounded))<16384,bounded
        return {"readbacks":observations,"bounded":bounded}

    def dm_creatures(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.until(lambda:self.creature(actor) is not None)
        observations={"actor":actor}
        for op,getter in [("set_invulnerable","GetPlotFlag"),("set_immortal","GetImmortal")]:
            flags=[]
            for enabled in [True,False]:
                self.command(op=op,target=actor,enabled=enabled)
                value=self.native("JsonBool("+getter+"(OBJECT_SELF))",actor)
                assert value==enabled,(op,value);flags.append(value)
            self.command(expect="invalid",op=op,target=actor,enabled=1)
            observations[op]=flags
        ai=[]
        for level,value in [("very_low",0),("low",1),("normal",2),("high",3),("very_high",4)]:
            self.command(op="set_ai",target=actor,level=level)
            observed=self.native("JsonInt(GetAILevel(OBJECT_SELF))",actor)
            constant=self.native("JsonInt(AI_LEVEL_"+level.upper()+")",actor)
            immediate=self.command(op="run_chunk",target=actor,code="SetAILevel(OBJECT_SELF, AI_LEVEL_"+level.upper()+"); MgReturn(JsonDump(JsonInt(GetAILevel(OBJECT_SELF))));",**{"return":True})["value"]
            ai.append({"name":level,"constant":constant,"next_tick":observed,"immediate":immediate})
            if level=="very_high":
                code='#include "nwnx_object"'+chr(10)+'void main() { NWNX_Object_SetAILevel(OBJECT_SELF, AI_LEVEL_VERY_HIGH); MgReturn(JsonDump(JsonInt(GetAILevel(OBJECT_SELF)))); }'
                ai[-1]["nwnx_immediate"]=self.command(op="run_chunk",target=actor,code=code,wrap=False,**{"return":True})["value"]
            assert observed==constant or (level=="very_high" and immediate==observed==3),(level,constant,observed,immediate)
        self.command(op="set_ai",target=actor,level="default")
        ai.append(["default",self.native("JsonInt(GetAILevel(OBJECT_SELF))",actor)])
        self.command(expect="invalid",op="set_ai",target=actor,level="always")
        observations["ai"]=ai
        damage=f'object o=StringToObject("{actor}"); ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectDamage(GetCurrentHitPoints(o)-1, DAMAGE_TYPE_MAGICAL), o);'
        self.chunk(damage)
        self.until(lambda:self.creature(actor)["hp"][0]==1)
        self.command(op="heal",target=actor,amount=2)
        self.until(lambda:self.creature(actor)["hp"][0]==3)
        observations["heal_partial"]=self.creature(actor)
        self.command(op="heal",target=actor)
        maximum=self.native("JsonInt(GetMaxHitPoints(OBJECT_SELF))",actor)
        self.until(lambda:self.creature(actor)["hp"][0]==maximum)
        observations["heal_full"]=self.creature(actor)
        self.chunk(damage)
        self.until(lambda:self.creature(actor)["hp"][0]==1)
        self.command(op="force_rest",target=actor)
        self.until(lambda:self.creature(actor)["hp"][0]==maximum)
        observations["rest"]=self.creature(actor)
        self.command(op="limbo",target=actor)
        self.until(lambda:self.creature(actor) is None)
        valid=self.native('JsonBool(GetIsObjectValid(StringToObject("'+actor+'")) && !GetIsObjectValid(GetArea(StringToObject("'+actor+'"))))')
        assert valid;observations["limbo_valid_without_area"]=valid
        self.command(op="unlimbo",target=actor,x=12,y=8,z=0)
        self.until(lambda:self.creature(actor) is not None and abs(self.creature(actor)["x"]-12)<.2 and abs(self.creature(actor)["y"]-8)<.2)
        observations["returned"]=self.creature(actor)
        self.command(expect="invalid",op="unlimbo",target=actor,x=12,y=8,z=0)
        self.command(op="run_chunk",target=actor,code="SetIsDestroyable(FALSE); SetEventScript(OBJECT_SELF, EVENT_SCRIPT_CREATURE_ON_DEATH, \"\");")
        self.command(op="kill",target=actor)
        self.until(lambda:self.creature(actor) is not None and self.creature(actor)["dead"])
        assert self.native('JsonBool(GetIsObjectValid(StringToObject("'+actor+'")))')
        observations["dead_corpse"]=self.creature(actor)
        for op in ["kill","heal","force_rest","limbo"]:self.command(expect="dead",op=op,target=actor)
        for op in ["kill","heal","force_rest","limbo"]:self.command(expect="not_found",op=op,target="7ffffffe")
        other=self.command(op="spawn",kind="creature",r="nw_bartender",x=2,y=2,z=0,f=0)["obj"]
        for amount in [0,100001,True,1.5]:self.command(expect="invalid",op="heal",target=other,amount=amount)
        return observations

    def set_facing(self):
        snapshots=[]
        objects=self.furniture()
        for actor in objects:
            before=self.object(actor)
            for bad in [-1,360,True,"90",None]:
                self.command(expect="invalid",op="set_facing",target=actor,f=bad)
            self.command(expect="disabled" if actor==objects[1] else "ok",op="set_facing",target=actor,f=123)
            if actor==objects[0]:
                self.until(lambda:abs(self.object(actor)["f"]-123)<0.1)
            else:
                seq=self.state["seq"]
                self.until(lambda:self.state["seq"]>=seq+5)
                assert self.object(actor)["f"]==before["f"]
            snapshots.append({"before":before,"after":self.object(actor)})
        return {"objects":snapshots}

    def visual_transform(self):
        snapshots=[]
        for actor in self.furniture():
            before=self.object(actor)
            assert "vt" not in before,before
            values={"scale":1.5,"rotate":{"x":10,"y":20,"z":30},"translate":{"x":1,"y":2,"z":3}}
            self.command(op="set_visual_transform",target=actor,**values)
            self.until(lambda:self.object(actor).get("vt")==values)
            after=self.object(actor)
            assert all(after[k]==before[k] for k in ["x","y","z","f"]),(before,after)
            for bad in [{"scale":0},{"scale":11},{"scale":True},{"rotate":{"x":0}},
                        {"rotate":{"x":361,"y":0,"z":0}},{"translate":{"x":0,"y":0,"z":1001}},
                        {"reset":1},{"reset":True,"scale":2},{}]:
                self.command(expect="invalid",op="set_visual_transform",target=actor,**bad)
            assert self.object(actor)["vt"]==values
            self.command(op="set_visual_transform",target=actor,scale=2)
            self.until(lambda:self.object(actor).get("vt",{}).get("scale")==2)
            assert self.object(actor)["vt"]["rotate"]==values["rotate"]
            self.command(op="set_visual_transform",target=actor,reset=True)
            self.until(lambda:"vt" not in self.object(actor))
            snapshots.append({"before":before,"after":after,"reset":self.object(actor)})
        return {"objects":snapshots,"limitation":"Native transform state proved; no client rendering/collision claim."}

    def set_position(self):
        snapshots=[]
        for actor in self.furniture():
            before=self.object(actor)
            for bad in [True,"15",None]:
                self.command(expect="invalid",op="set_position",target=actor,x=bad,y=15,z=2)
            self.command(op="set_position",target=actor,x=15,y=15,z=2)
            self.until(lambda:all(abs(self.object(actor)[k]-v)<0.01 for k,v in {"x":15,"y":15,"z":2}.items()))
            after=self.object(actor)
            assert after["f"]==before["f"]
            snapshots.append({"before":before,"after":after})
        return {"objects":snapshots,"limitation":"Native position proved; client collision/pathing requires client proof."}

    def play_animation(self):
        chest,door=self.furniture()
        snapshots=[]
        for name,expected in [("activate",73),("deactivate",74),("open",75),("close",76)]:
            self.command(op="play_animation",target=chest,name=name)
            self.until(lambda:self.object(chest).get("anim")==expected)
            snapshots.append({"name":name,"object":self.object(chest)})
        for name in ["open","close"]:
            self.command(op="play_animation",target=door,name=name)
            self.until(lambda:self.object(door)["open"]==(name=="open"))
            snapshots.append({"name":name,"object":self.object(door)})
        for name in ["activate","deactivate","dance",True]:
            self.command(expect="invalid",op="play_animation",target=door,name=name)
        self.chunk(f'SetLocked(StringToObject("{door}"),TRUE);')
        self.command(expect="invalid",op="play_animation",target=door,name="open")
        return {"animations":snapshots,"limitation":"Native animation/open state; no rendered frames."}

    def set_useable(self):
        snapshots=[]
        for actor in self.furniture():
            before=self.object(actor)
            for bad in [0,1,"true",None]:
                self.command(expect="invalid",op="set_useable",target=actor,useable=bad)
            changed=[]
            for value in [not before["useable"],before["useable"]]:
                self.command(op="set_useable",target=actor,useable=value)
                self.command(op="run_script",target=actor,script="mget_useable")
                self.until(lambda:self.object(actor)["useable"]==value)
                self.until(lambda:'Useable={"type":"byte","value":'+str(int(value))+'}' in self.object(actor)["name"])
                changed.append(self.object(actor))
            snapshots.append({"before":before,"changes":changed})
        return {"objects":snapshots}

    def set_name(self):
        objects=self.furniture()
        objects.append(self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"])
        snapshots=[]
        for actor in objects:
            observe=lambda:self.creature(actor) or self.object(actor)
            before=observe()
            for bad in [""," \t\r\n", "a"*201, True, 25]:
                self.command(expect="invalid",op="set_name",target=actor,name=bad)
            name='Renamed "quoted" \\ '+actor
            self.command(op="set_name",target=actor,name=name)
            self.until(lambda:observe()["name"]==name)
            snapshots.append({"before":before,"after":observe()})
        return {"objects":snapshots}

    def polish(self):
        phrase="Za\u017c\xf3\u0142\u0107 g\u0119\u015bl\u0105 ja\u017a\u0144"
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        observed=[]
        for escaped in [False,True]:
            self.command(op="set_name",target=actor,name=phrase,wire_ascii=escaped)
            self.until(lambda:self.creature(actor)["name"]!="Bartender" and not self.creature(actor)["name"].startswith("native-bytes="))
            named=self.creature(actor)
            seq=max((e["seq"] for e in self.events.values()),default=0)
            self.command(op="npc_say",target=actor,text=phrase,volume="talk",wire_ascii=escaped)
            self.until(lambda:any(e.get("id")==actor and e["seq"]>seq for e in self.matching("chat")))
            chat=next(e for e in self.matching("chat") if e.get("id")==actor and e["seq"]>seq)
            assert chat["text"]==named["name"] and chat["name"]==named["name"] and chat["channel"]==1,chat
            self.command(op="run_script",target=actor,script="mget_textbytes")
            self.until(lambda:self.creature(actor)["name"].startswith("native-bytes="))
            observed.append({"escaped_wire":escaped,"named":named,"chat":chat,"native_length":self.creature(actor)["name"]})
        for value in [" \t\r\n", "\t", "\r"]:
            self.command(expect="invalid",op="npc_say",target=actor,text=value)
        assert observed[0]["named"]["name"]!=phrase and observed[0]["native_length"]=="native-bytes=26"
        assert observed[1]["native_length"]=="native-bytes=17"
        return {"input":phrase,"utf8_hex":phrase.encode().hex(),"observed":observed,
                "limitation":"Encoding diagnosis; escaped JSON also needs a Polish-capable game codepage. No client rendering claim."}

    def polish_cp1250(self):
        result=self.polish()
        assert result["observed"][1]["named"]["name"]==result["input"]
        assert result["observed"][1]["chat"]["text"]==result["input"]
        result["proven_route"]="ASCII-escaped Unicode JSON over Redis with server game.language.codepage=cp1250"
        return result

    def mixed_speech(self):
        return self.rust_api("mixed_speech")

    def mixed_speech_cp1250(self):
        return self.mixed_speech_repro()

    def mixed_speech_repro(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        observed=[]
        for stage in range(4):
            self.chunk(f'SetLocalInt(GetModule(), "MGET_MIXED_STAGE", {stage});')
            seq=max((e["seq"] for e in self.events.values()),default=0)
            self.command(op="run_script",target=actor,script="mget_mixed")
            self.until(lambda:any(e.get("id")==actor and e["seq"]>seq for e in self.matching("chat")))
            observed.append({"stage":stage,"bytes":self.native('JsonParse(GetLocalString(GetModule(), "MGET_MIXED_PROBE"))'),
                             "chat":next(e for e in self.matching("chat") if e.get("id")==actor and e["seq"]>seq)})
        return {"observed":observed}

    def module_polish_repro(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        self.command(op="run_script",target=actor,script="mget_polish")
        self.until(lambda:any(e.get("id")==actor and e.get("text","").startswith("Naprawd") for e in self.matching("chat")))
        self.until(lambda:any("MGET_POLISH" in e["line"] for e in self.logs))
        chat=next(e for e in self.matching("chat") if e.get("id")==actor and e.get("text","").startswith("Naprawd"))
        native=self.native('JsonInt(GetLocalInt(GetModule(),"MGET_POLISH_BYTES"))')
        raw=(SCRATCH/"logs.0/nwserverLog1.txt").read_bytes().split(b"MGET_POLISH ")[-1].splitlines()[0]
        return {"source":"module custom TLK line 16777216, referenced by mget_polish.dlg",
                "source_cp1250_hex":polish_tlk_bytes().hex(),
                "native_byte_length":native,"native_log_hex":raw.hex(),"chat":chat,
                "report":self.creature(actor),"log":next(e for e in self.logs if "MGET_POLISH" in e["line"])}

    def module_polish(self):
        return self.rust_api("module_polish")

    def move_hold(self):
        actor = self.command(op="spawn", kind="creature", r="nw_bartender", x=3, y=5, z=0, f=0)["obj"]
        self.command(op="run_script", target=actor, script="mget_control")
        self.command(op="npc_move", target=actor, x=17, y=5, z=0, hold=True, timeout=2)
        self.until(lambda: self.creature(actor)["name"] == "commandable=0" and self.creature(actor)["x"] > 3.3)
        held = self.creature(actor)
        self.until(lambda: self.creature(actor)["name"] == "commandable=1")
        timeout = self.creature(actor)
        self.command(op="npc_move", target=actor, x=3, y=5, z=0, hold=True, timeout=30)
        self.until(lambda: self.creature(actor)["name"] == "commandable=0")
        self.until(lambda: abs(self.creature(actor)["x"] - 3) < 1 and self.creature(actor)["name"] == "commandable=1")
        arrival = self.creature(actor)
        for extra in [{"hold": 1}, {"timeout": 0}, {"timeout": 121}, {"timeout": 1.5}, {"run": "yes"}]:
            self.command(expect="invalid", op="npc_move", target=actor, x=17, y=5, z=0, **extra)
        return {"held_and_moved": held, "timeout_restored": timeout, "arrival_restored": arrival,
                "invalid_types_and_bounds": True}

    def pc_stop_combat(self):
        # NPC stand-ins have their stock AI scripts removed, like a player's
        # lack of NPC auto-attack scripts. Never fake GetIsPC or bypass its gate.
        def standin(resref,x):
            source=self.command(op="spawn",kind="creature",r=resref,x=x,y=5,z=0,f=0)["obj"]
            template=self.command(op="capture_object",target=source)["object_json"]
            cleared=[]
            for key,value in template.items():
                if key.startswith("Script") and isinstance(value,dict) and value.get("type")=="resref":
                    value["value"]="";cleared.append(key)
            assert len(cleared)>=10,cleared
            actor=self.command(op="spawn_json",json=template,x=x,y=5,z=0,f=0)["obj"]
            self.command(op="destroy",target=source)
            self.command(op="set_invulnerable",target=actor,enabled=True)
            return actor,cleared
        actor,scripts=standin("nw_bandit001",3)
        victim,_=standin("nw_bartender",5)
        self.chunk(f'object a=StringToObject("{actor}"); object v=StringToObject("{victim}"); SetLocalObject(GetModule(),"MGET_PCSTOP_ACTOR",a); SetLocalObject(GetModule(),"MGET_PCSTOP_VICTIM",v); AssignCommand(v,ClearAllActions(TRUE)); SetCommandable(FALSE,v);')
        self.command(op="npc_attack",target=actor,victim=victim,hold=True)
        self.until(lambda:self.creature(actor)["combat"] and self.creature(actor).get("target")==victim)
        active=self.creature(actor)
        # Run on the area, so run_script's generic target cleanup cannot cancel
        # the held attack before the exact production PC helper is exercised.
        self.command(op="run_script",script="mget_pcstop")
        before=self.native('GetLocalJson(GetModule(),"MGET_PCSTOP_BEFORE")')
        after=self.native('GetLocalJson(GetModule(),"MGET_PCSTOP_AFTER")')
        assert before["combat"] and before["attack_target"] and before["held"]>0 and not before["commandable"],before
        assert not after["attack_target"] and after["action"]==65535 and after["held"]==0 and after["commandable"],after
        for key in ["faction","victim_faction","rep_actor","rep_victim"]: assert before[key]==after[key],(key,before,after)
        end=self.state["seq"]+5
        self.until(lambda:self.state["seq"]>=end)
        row=self.creature(actor)
        assert row["act"]==65535 and not row.get("target") and not row["combat"],row
        self.command(op="npc_move",target=actor,x=3,y=8,z=0,hold=False,run=False)
        self.until(lambda:abs(self.creature(actor)["x"]-3)<0.6 and abs(self.creature(actor)["y"]-8)<0.6)
        return {"npc_standin":actor,"victim":victim,"disabled_npc_ai_scripts":scripts,"active_report":active,"native_before":before,"native_after":after,"report_after_five_ticks":row,"next_order_works":True,
                "limitation":"No connected PC. The exact production PC action helper is proven on an NPC stand-in; native GetIsPC dispatch and connected-player UI response require the owner client. Rust tests prove local/scope refusal and PC command JSON."}

    def stop_combat_target(self):
        def spawn(r,x):
            actor=self.command(op="spawn",kind="creature",r=r,x=x,y=5,z=0,f=0)["obj"]
            self.command(op="set_invulnerable",target=actor,enabled=True)
            return actor
        actor, victim = spawn("nw_bandit001",3), spawn("nw_bartender",5)
        other, other_victim = spawn("nw_bandit001",14), spawn("nw_bartender",16)
        self.command(op="npc_attack",target=actor,victim=victim,hold=True)
        self.command(op="npc_attack",target=other,victim=other_victim,hold=True)
        self.until(lambda:all(self.creature(o)["combat"] for o in [actor,other]))
        before=[self.creature(o) for o in [actor,other]]
        result=self.command(op="stop_combat",target=actor)
        assert result["affected"]==[actor],result
        self.until(lambda:not self.creature(actor)["combat"] and not self.creature(actor).get("target"))
        end=self.state["seq"]+6
        self.until(lambda:self.state["seq"]>=end)
        after=[self.creature(o) for o in [actor,other]]
        diagnostic=self.command(op="run_chunk",code=f'object a=StringToObject("{actor}"); object v=StringToObject("{victim}"); json p=JsonArray(); p=JsonArrayInsert(p,JsonInt(GetReputation(v,a))); p=JsonArrayInsert(p,JsonInt(GetReputation(a,v))); p=JsonArrayInsert(p,JsonBool(GetIsEnemy(v,a))); p=JsonArrayInsert(p,JsonBool(GetIsEnemy(a,v))); p=JsonArrayInsert(p,JsonBool(GetCommandable(v))); p=JsonArrayInsert(p,JsonInt(GetCurrentAction(v))); p=JsonArrayInsert(p,JsonString(ObjectToString(GetAttackTarget(v)))); MgReturn(JsonDump(p));',**{"return":True})["value"]
        # GetIsInCombat can stay true while an unselected enemy attacks this
        # actor. Its own queue and target must remain cleared throughout calm.
        assert after[0]["act"]==65535 and not after[0].get("target"),(after,diagnostic)
        assert after[1]["combat"] and after[1].get("target")==other_victim,after
        assert not self.native(f'JsonBool(GetCommandable(StringToObject("{actor}")))')
        for fields in [{},{"x":3},{"x":3,"y":5},{"radius":4},{"target":actor,"x":3},
                       {"target":actor,"radius":4},{"target":actor,"x":3,"y":5,"radius":4},
                       {"target":"bad-id"},{"target":1}]:
            self.command(expect="invalid",op="stop_combat",**fields)
        self.command(expect="not_found",op="stop_combat",target="7ffffffe")
        door=self.native('JsonString(ObjectToString(GetObjectByTag("mget_door")))')
        self.command(expect="invalid",op="stop_combat",target=door)
        radius=self.command(op="stop_combat",x=15,y=5,radius=4)
        assert set(radius["affected"])=={other,other_victim},radius
        self.until(lambda:all(not self.creature(o)["combat"] for o in [other,other_victim]))
        self.command(op="npc_move",target=actor,x=2,y=5,z=0,hold=True)
        self.until(lambda:abs(self.creature(actor)["x"]-2)<.6 and abs(self.creature(actor)["y"]-5)<.6)
        self.until(lambda:self.state["seq"]>=end+8)
        assert self.native(f'JsonBool(GetCommandable(StringToObject("{actor}")))')
        self.command(op="run_chunk",target=actor,code='SetIsDestroyable(FALSE); SetEventScript(OBJECT_SELF, EVENT_SCRIPT_CREATURE_ON_DEATH, "");')
        self.command(op="kill",target=actor)
        self.until(lambda:self.creature(actor)["dead"])
        self.command(expect="dead",op="stop_combat",target=actor)
        return {"before":before,"target_result":result,"after_six_ticks":after,"radius_result":radius,"native_relationships":diagnostic,"new_move_released_calm_hold":True,
                "invalid_selection_cases":9,"dead_type_missing_refusals":True,
                "limitation":"New orders or hostile scripts may restart combat. PC protection is covered by Rust tests and game guards; no connected PC in this private proof."}

    def stop_combat(self):
        spawn = lambda r,x,y: self.command(op="spawn", kind="creature", r=r, x=x, y=y, z=0, f=0)["obj"]
        bandit, victim, outside = spawn("nw_bandit001",8,10), spawn("nw_bartender",10,10), spawn("nw_bartender",19,19)
        self.chunk(f'SetImmortal(StringToObject("{bandit}"), TRUE); SetImmortal(StringToObject("{victim}"), TRUE);')
        self.command(op="npc_attack", target=bandit, victim=victim, hold=True)
        self.until(lambda: self.creature(bandit)["combat"] and self.creature(bandit).get("target") == victim)
        before = self.creature(bandit)
        result = self.command(op="stop_combat", x=9, y=10, radius=4)
        assert set(result["affected"]) == {bandit,victim}, result
        self.chunk(f'SetLocalObject(GetModule(), "MGET_CALM_OTHER", StringToObject("{victim}"));')
        self.command(op="run_script", target=bandit, script="mget_calm")
        self.until(lambda: self.creature(bandit)["name"].startswith("rep="))
        assert "enemy=0" in self.creature(bandit)["name"], self.creature(bandit)
        self.until(lambda: all(not self.creature(o)["combat"] and not self.creature(o).get("target") for o in [bandit,victim]), 15)
        end = self.state["seq"] + 7
        self.until(lambda: self.state["seq"] >= end)
        assert all(not self.creature(o)["combat"] for o in [bandit,victim])
        self.chunk(f'SetLocalObject(GetModule(), "MGET_CALM_OTHER", StringToObject("{bandit}"));')
        self.command(op="run_script", target=outside, script="mget_calm")
        self.until(lambda: self.creature(outside)["name"].startswith("rep="))
        assert self.creature(outside)["name"].startswith("rep=0 "), self.creature(outside)
        for fields in [{"radius":0}, {"radius":51}, {"radius":"4"}, {"x":"9"}]:
            args=dict(x=9,y=10,radius=4);args.update(fields)
            self.command(expect="invalid",op="stop_combat",**args)
        return {"before":before,"result":result,"after":[self.creature(o) for o in [bandit,victim]],
                "outside":self.creature(outside),"calm_across_later_ticks":True}

    def server_factions(self):
        rows = self.state.get("factions")
        expected = ["PC", "Hostile", "Commoner", "Merchant", "Defender", "Agents", "Session 1", "Session 2", "test"]
        assert rows and [(r["id"], r["name"]) for r in rows] == list(enumerate(expected)), rows
        native = self.native('JsonObjectGet(JsonObjectGet(TemplateToJson("repute", RESTYPE_FAC), "FactionList"), "value")')
        assert [r["FactionName"]["value"] for r in native] == expected, native
        actor = self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(op="set_faction",target=actor,faction=6)
        self.until(lambda:self.creature(actor)["faction"]==6)
        # This ID could exist in an unsaved editor module, but is absent here.
        refused = self.command(expect="invalid",op="set_faction",target=actor,faction=9)
        assert self.creature(actor)["faction"] == 6
        return {"factions":rows,"native_names":expected,"custom_faction_report":self.creature(actor),"absent_id":refused}

    def set_faction(self):
        actor = self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        observed = []
        for value,expected in [("hostile",1),("merchant",3),("defender",4),(2,2)]:
            result=self.command(op="set_faction",target=actor,faction=value)
            assert result["faction"] == expected
            self.command(op="run_script",target=actor,script="mget_faction")
            self.until(lambda:self.creature(actor)["faction"]==expected and self.creature(actor)["name"]==f"native-faction={expected}")
            observed.append(self.creature(actor))
        for bad in ["bad",True,-1,65536,1.5,99]:
            self.command(expect="invalid",op="set_faction",target=actor,faction=bad)
            assert self.creature(actor)["faction"]==2
        self.command(expect="not_found",op="set_faction",target="7ffffffe",faction=1)
        self.chunk(f'ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectDeath(), StringToObject("{actor}"));')
        self.until(lambda:self.creature(actor)["dead"])
        self.command(expect="dead",op="set_faction",target=actor,faction=1)
        return {"observed":observed,"invalid_and_dead_refusals":True}

    def freeze(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(op="freeze",target=actor,seconds=30)
        self.until(lambda:self.creature(actor)["frozen"])
        self.command(op="npc_move",target=actor,x=17,y=5,z=0,hold=True)
        end=self.state["seq"]+2
        self.until(lambda:self.state["seq"]>=end)
        assert abs(self.creature(actor)["x"]-3)<.1,self.creature(actor)
        held=self.creature(actor)
        self.chunk('SetLocalString(GetModule(), "MGET_FREEZE_PROBE", "apply");')
        self.command(op="run_script",target=actor,script="mget_freeze")
        self.command(op="unfreeze",target=actor)
        self.chunk('SetLocalString(GetModule(), "MGET_FREEZE_PROBE", "probe");')
        self.command(op="run_script",target=actor,script="mget_freeze")
        self.until(lambda:self.creature(actor)["name"]=="freeze=0 other=1")
        assert not self.creature(actor)["frozen"]
        preserved=self.creature(actor)
        self.chunk('SetLocalString(GetModule(), "MGET_FREEZE_PROBE", "remove");')
        self.command(op="run_script",target=actor,script="mget_freeze")
        self.command(op="npc_move",target=actor,x=17,y=5,z=0,hold=True)
        self.until(lambda:self.creature(actor)["x"]>4)
        moving=self.creature(actor)
        self.command(op="freeze",target=actor,seconds=3)
        self.until(lambda:self.creature(actor)["frozen"])
        self.until(lambda:not self.creature(actor)["frozen"])
        for bad in [0,-1,301,"3",True]:
            self.command(expect="invalid",op="freeze",target=actor,seconds=bad)
        return {"held_position":held,"other_tag_preserved":preserved,"moving_after_unfreeze":moving,"timed_expiry":True}

    def speech_volume(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        observed=[]
        for volume,channel in [("talk",1),("whisper",3),("shout",2)]:
            line="volume:"+volume
            self.command(op="npc_say",target=actor,text=line,volume=volume)
            self.until(lambda:any(e.get("text")==line and e["id"]==actor for e in self.matching("chat")))
            event=next(e for e in self.matching("chat") if e.get("text")==line)
            assert event["channel"]==channel,event
            observed.append(event)
        for bad in ["bad",1,False]:
            self.command(expect="invalid",op="npc_say",target=actor,text="refused",volume=bad)
        return {"speech":observed,"invalid_volume_refused":True}

    def spawn_many(self):
        base=dict(op="spawn",kind="creature",r="nw_bartender",x=10,y=10,z=0,f=45)
        result=self.command(**base,count=10,faction="merchant")
        ids=result["objects"]
        assert len(ids)==10 and len(set(ids))==10 and result["obj"]==ids[0] and result["count"]==10
        self.until(lambda:all(self.creature(o) is not None for o in ids))
        actors=[self.creature(o) for o in ids]
        assert all(o["own"] and o["faction"]==3 for o in actors)
        distances=[((a["x"]-b["x"])**2+(a["y"]-b["y"])**2)**.5 for i,a in enumerate(actors) for b in actors[i+1:]]
        # CreateObject can adjust a requested point to the tile walkmesh.
        assert min(distances)>1.0,actors
        for bad in [0,11,1.5,True,"2"]:
            self.command(expect="invalid",**base,count=bad)
        for bad in ["unknown",99]:
            self.command(expect="invalid",**base,count=3,faction=bad)
        missing=dict(base,r="mget_missing")
        self.command(expect="not_found",**missing)
        end=self.state["seq"]+2;self.until(lambda:self.state["seq"]>=end)
        assert {o["id"] for a in self.state["areas"] for o in a["cre"]}==set(ids)
        return {"result":result,"actors":actors,"minimum_separation":min(distances),"refusals_leave_no_extra_creatures":True}

    def players_disabled(self):
        results=[]
        for op in ["pc_heal","pc_raise","pc_teleport","pc_give_gold","pc_give_xp","pc_give_item"]:
            results.append(self.command(expect="disabled",op=op,target="7ffffffe",players=True))
        return {"server_switch_refusals":results}

    def pc_lifecycle(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=5,z=0,f=0)["obj"]
        self.command(op="run_chunk",target=actor,code='SetIsDestroyable(FALSE); SetEventScript(OBJECT_SELF, EVENT_SCRIPT_CREATURE_ON_DEATH, "");')
        self.command(expect="disabled",op="pc_heal",target=actor)
        self.command(expect="invalid",op="pc_raise",target=actor,players=True)
        self.command(op="pc_heal",target=actor,players=True)
        alive=self.creature(actor)
        self.command(op="kill",target=actor,players=True)
        self.until(lambda:self.creature(actor)["dead"])
        dead=self.creature(actor)
        for op in ["pc_heal","heal","kill"]:self.command(expect="dead",op=op,target=actor,players=True)
        self.command(op="pc_raise",target=actor,players=True)
        self.until(lambda:not self.creature(actor)["dead"] and self.creature(actor)["hp"][0]==self.creature(actor)["hp"][1])
        raised=self.creature(actor)
        self.command(expect="invalid",op="pc_raise",target=actor,players=True)
        return {"living":alive,"dead":dead,"raised":raised,"limitation":"NPC stand-in. No client connected: real PC OnDeath/OnRespawn module scripts and client presentation remain unverified."}

    def pc_heal(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        self.command(expect="disabled",op="pc_heal",target=actor)
        self.command(expect="disabled",op="pc_heal",target=actor,players=False)
        self.command(expect="invalid",op="pc_heal",target=actor,players=1)
        snapshots=[]
        for extra in [{"amount":1},{}]:
            self.chunk(f'ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectDamage(1), StringToObject("{actor}"));')
            self.until(lambda:self.creature(actor)["hp"][0]<self.creature(actor)["hp"][1])
            before=self.creature(actor)
            self.command(op="pc_heal",target=actor,players=True,**extra)
            self.until(lambda:self.creature(actor)["hp"][0]==self.creature(actor)["hp"][1])
            snapshots.append({"before":before,"after":self.creature(actor)})
        for bad in [0,-1,100001,True,1.5]:
            self.command(expect="invalid",op="pc_heal",target=actor,players=True,amount=bad)
        return {"heals":snapshots,"command_switch_refusals":True,"limitation":"Owned NPC stand-in; real client/master protection awaits client proof."}

    def pc_raise(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        self.command(expect="disabled",op="pc_raise",target=actor)
        self.command(expect="invalid",op="pc_raise",target=actor,players=True)
        self.chunk(f'ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectDeath(), StringToObject("{actor}"));')
        self.until(lambda:self.creature(actor)["dead"])
        before=self.creature(actor)
        self.command(expect="dead",op="pc_heal",target=actor,players=True)
        self.command(op="pc_raise",target=actor,players=True)
        self.until(lambda:not self.creature(actor)["dead"] and self.creature(actor)["hp"][0]==self.creature(actor)["hp"][1])
        return {"before":before,"after":self.creature(actor),"limitation":"Owned NPC stand-in; actual PC/client needs client proof."}

    def pc_teleport(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=45)["obj"]
        self.command(expect="disabled",op="pc_teleport",target=actor,x=15,y=15,z=0)
        for bad in [True,"15",None]:
            self.command(expect="invalid",op="pc_teleport",target=actor,players=True,x=bad,y=15,z=0)
        self.command(expect="not_found",op="pc_teleport",area="absent",target=actor,players=True,x=15,y=15,z=0)
        before=self.creature(actor)
        self.command(op="pc_teleport",target=actor,players=True,x=15,y=15,z=0)
        self.until(lambda:abs(self.creature(actor)["x"]-15)<0.2 and abs(self.creature(actor)["y"]-15)<0.2)
        after=self.creature(actor)
        assert abs(after["f"]-before["f"])<0.1 and abs(after["z"]-before["z"])<0.2,after
        return {"before":before,"after":after,"limitation":"Owned NPC stand-in; same-area only."}

    def pc_give_gold(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        self.command(expect="disabled",op="pc_give_gold",target=actor,amount=25)
        for bad in [0,-1,1000001,True,1.5,"25",None]:
            self.command(expect="invalid",op="pc_give_gold",target=actor,players=True,amount=bad)
        before=self.creature(actor)
        self.command(op="pc_give_gold",target=actor,players=True,amount=25)
        self.until(lambda:self.creature(actor)["gold"]==before["gold"]+25)
        return {"before":before,"after":self.creature(actor),"limitation":"Owned NPC stand-in."}

    def pc_give_xp(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        self.command(expect="disabled",op="pc_give_xp",target=actor,amount=25)
        for bad in [0,-1,100001,True,1.5,"25",None]:
            self.command(expect="invalid",op="pc_give_xp",target=actor,players=True,amount=bad)
        before=self.creature(actor)
        self.command(op="pc_give_xp",target=actor,players=True,amount=25)
        self.until(lambda:self.creature(actor)["xp"]==before["xp"]+25)
        return {"before":before,"after":self.creature(actor),"limitation":"Owned NPC stand-in."}

    def pc_give_item(self):
        actor=self.command(op="spawn",kind="creature",r="nw_bartender",x=3,y=3,z=0,f=0)["obj"]
        self.command(expect="disabled",op="pc_give_item",target=actor,resref="nw_it_mpotion001")
        for bad in ["", "a"*17, True, "../bad"]:
            self.command(expect="invalid",op="pc_give_item",target=actor,players=True,resref=bad)
        self.command(expect="not_found",op="pc_give_item",target=actor,players=True,resref="mget_absent")
        self.command(op="run_script",target=actor,script="mget_inventory")
        self.until(lambda:self.creature(actor)["name"].startswith("potions="))
        before=self.creature(actor)
        count=int(before["name"].split()[0].split("=")[1])
        result=self.command(op="pc_give_item",target=actor,players=True,resref="nw_it_mpotion001")
        self.command(op="run_script",target=actor,script="mget_inventory")
        self.until(lambda:self.creature(actor)["name"]==f'potions={count+1} item={result["obj"]}')
        return {"before":before,"after":self.creature(actor),"result":result,"limitation":"Owned NPC inventory stand-in."}

    def vfx(self):
        actor = self.command(op="spawn", kind="creature", r="nw_bartender", x=3, y=3, z=0, f=0)["obj"]
        self.until(lambda: self.creature(actor) is not None)
        assert self.creature(actor)["vfx"] == []
        self.command(op="run_script", target=actor, script="mget_vfx")
        self.until(lambda: self.creature(actor)["name"].startswith("vfx-probe"))
        permanent = self.creature(actor)
        assert permanent["name"] == "vfx-probe 411 0 0 0 0 0 0 0", permanent
        assert permanent["vfx"] == [411], permanent
        self.chunk('SetLocalInt(GetModule(), "MGET_VFX_STAGE", 1);')
        self.command(op="run_script", target=actor, script="mget_vfx")
        self.until(lambda: self.creature(actor)["name"].startswith("vfx-temporary"))
        temporary = self.creature(actor)
        assert temporary["vfx"] == [int(temporary["name"].split()[1])], temporary
        self.chunk('SetLocalInt(GetModule(), "MGET_VFX_STAGE", 3);')
        self.command(op="run_script", target=actor, script="mget_vfx")
        self.until(lambda: self.creature(actor)["name"].startswith("vfx-count"))
        capped = self.creature(actor)
        assert capped["name"] == "vfx-count 17" and capped["vfx"] == [411] * 16, capped
        self.chunk('SetLocalInt(GetModule(), "MGET_VFX_STAGE", 2);')
        self.command(op="run_script", target=actor, script="mget_vfx")
        self.until(lambda: self.creature(actor)["vfx"] == [])
        spell = self.spell(projectile=True)
        self.until(lambda: any(e["id"] == spell["id"] and e["spell"] == 107 for e in self.matching("vfx")), 30)
        projectile = next(e for e in self.matching("vfx") if e["id"] == spell["id"] and e["spell"] == 107)
        assert projectile["target"] == spell["target"] and projectile["subtype"] == "projectile", projectile
        assert not spell["instant"] and projectile["delta_ms"] >= 0, (spell, projectile)
        self.chunk('SetLocalInt(GetModule(), "MGET_CAST_LOCATION", TRUE);')
        self.command(op="run_script", target=spell["id"], script="mget_cast")
        self.until(lambda: any(e["id"] == spell["id"] and e["spell"] == 58 for e in self.matching("spell")), 30)
        location_spell = next(e for e in self.matching("spell") if e["id"] == spell["id"] and e["spell"] == 58)
        self.until(lambda: any(e["id"] == spell["id"] and e["spell"] == 58 for e in self.matching("vfx")), 30)
        location_projectile = next(e for e in self.matching("vfx") if e["id"] == spell["id"] and e["spell"] == 58)
        assert location_spell["target"] is None and location_projectile["target"] is None
        assert location_spell["position"] == location_projectile["position"], (location_spell, location_projectile)
        assert location_spell["position"]["x"] > 0 and location_spell["position"]["y"] > 0
        return {"permanent": permanent, "temporary_with_instant_applied": temporary,
                "cap": capped, "removal": [], "spell": spell, "projectile": projectile,
                "location_spell": location_spell, "location_projectile": location_projectile,
                "limitation": "No client render proof; general instant VFX application is outside capture scope."}

    def log(self):
        self.chunk('WriteTimestampedLogEntry("MGET_LOG plain");')
        self.until(lambda: any(e["line"].endswith("] MGET_LOG plain") for e in self.logs))
        plain = next(e for e in self.logs if e["line"].endswith("] MGET_LOG plain"))
        assert not plain["script_error"] and plain["boot"] == self.state["boot"]
        # Native VM errors may go to nwengineLog.txt, outside this helper's source.
        # Exercise the classifier with a clearly labeled server-written fixture.
        self.chunk('WriteTimestampedLogEntry("Script error: MGET_LOG classifier fixture");')
        self.until(lambda: any(e["script_error"] for e in self.logs))
        error = next(e for e in self.logs if e["script_error"])
        assert error["line"].endswith("] Script error: MGET_LOG classifier fixture")
        assert not any(e["script_error"] for e in self.logs if "MGLIVE: chunk" in e["line"])
        self.sub.close()
        self.pub.send("PUBLISH", PREFIX + ".cmd", json.dumps({"v": 1, "script_version": 2, "boot": self.state["boot"], "prefix": PREFIX, "commands": [{
            "id": "log-no-listener", "area": "events", "op": "run_chunk",
            "code": 'WriteTimestampedLogEntry("MGET_LOG retained");'}]}))
        assert self.pub.read() > 0
        time.sleep(2)
        self.sub = Redis()
        self.sub.send("SUBSCRIBE", PREFIX + ".state", PREFIX + ".log")
        self.sub.read()
        self.sub.read()
        self.until(lambda: any(e["line"].endswith("] MGET_LOG retained") for e in self.logs))
        # Rotate only this test-owned log; the server keeps its existing descriptor.
        path = SCRATCH / "logs.0/nwserverLog1.txt"
        rotated = SCRATCH / "logs.0/nwserverLog1.txt.tail-test-rotated"
        assert path.resolve() == path and not rotated.exists()
        path.rename(rotated)
        path.write_text("MGET_LOG rotation\n")
        self.until(lambda: any(e["line"] == "MGET_LOG rotation" for e in self.logs))
        path.write_text("")
        time.sleep(.3)
        path.write_text("MGET_LOG truncation\n")
        self.until(lambda: any(e["line"] == "MGET_LOG truncation" for e in self.logs))
        seqs = [e["seq"] for e in self.logs]
        assert seqs == sorted(set(seqs)) and len({e["stream"] for e in self.logs}) == 1
        return {"plain": plain, "classifier_fixture": error, "subscriber_gap": True,
                "rotation": True, "truncation": True,
                "limitation": "Classifier text was written by a test script, not raised by the VM; only nwserverLog1.txt is followed.",
                "initial_history_skipped": not any("Loading Module:" in e["line"] for e in self.logs)}


def cleanup_rotation(report):
    """Remove this completed proof's redundant rotation copy after all children exit."""
    path = SCRATCH / "logs.0/nwserverLog1.txt.tail-test-rotated"
    assert report["passed"] and all(p["exited"] for p in report["cleanup"])
    assert path.resolve() == path and path.is_file() and not path.is_symlink()
    assert report["boot"] in path.read_text(), "Rotation file belongs to a different run"
    entry = {"path": str(path), "logical_bytes": path.stat().st_size, "boot": report["boot"],
             "reason": "owned completed rotation fixture; observations retained in log JSON"}
    ledger_path = OUT / "cleanup.json"
    ledger = json.loads(ledger_path.read_text()) if ledger_path.exists() else {"task": "live-events", "deleted": []}
    path.unlink()
    ledger["deleted"].append(entry)
    dump(ledger_path, ledger)
    report["removed_artifacts"] = [entry]


def identity(pid):
    stat = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return {"pid": pid, "start": stat[19], "exe": os.readlink(f"/proc/{pid}/exe")}


def main():
    global OUT
    parser = argparse.ArgumentParser()
    parser.add_argument("--phase", choices=["watch_areas", "trigger_probe", "attribution", "capture_dialog", "spawn_world", "atmosphere", "patrol_obstacle", "pc_dm_chat", "equipment_look", "server_factions", "focus_chat", "focus_dialog", "custom_faction", "drop_flags", "session_factions", "vfx_transform", "inventory_actions", "restore_appearance", "drop_routine_probe", "mixed_speech", "mixed_speech_cp1250", "mixed_speech_repro", "inventory_grid", "creature_animation", "portrait", "economy", "pc_skill_rolls", "module_polish_repro", "module_polish", "animation_replacement", "pc_stop_combat", "teleport", "pc_force_bounds", "static_flag", "placeable_appearance", "lootable", "instance_appearance", "silent_chat", "pc_skill_target", "stop_combat_target", "tile", "patrol", "skill_check_disabled", "pc_skill_check", "pc_skill_nui", "capture", "capture_probe", "appearance", "apply_vfx", "pc_lifecycle", "kill_repro", "kill_override", "hostile_repro", "hostile_fight", "pc_dm_bounds", "dm_players_disabled", "pc_dm_players", "dm_traps", "dm_world", "pc_dm_world", "dm_inventory", "dm_variables", "dm_creatures", "chat", "combat", "spell", "log", "vfx", "move_hold", "stop_combat", "set_faction", "freeze", "speech_volume", "spawn_many", "players_disabled", "pc_heal", "pc_raise", "pc_teleport", "pc_give_gold", "pc_give_xp", "pc_give_item", "set_facing", "visual_transform", "set_position", "play_animation", "set_useable", "set_name", "polish", "polish_cp1250", "rust_api", "chunk_return", "delta"], default="chat")
    # The profiler reuses this exact private-server ownership/cleanup route.
    parser._option_string_actions["--phase"].choices.append("measure")
    parser._option_string_actions["--phase"].choices.append("chat_latency")
    parser._option_string_actions["--phase"].choices.append("pc_reply")
    parser._option_string_actions["--phase"].choices.append("redis_restart")
    parser._option_string_actions["--phase"].choices.append("stress")
    parser._option_string_actions["--phase"].choices.append("command_receipts")
    parser._option_string_actions["--phase"].choices.append("command_budget")
    parser._option_string_actions["--phase"].choices.append("command_accounting")
    parser._option_string_actions["--phase"].choices.append("command_expiry")
    parser._option_string_actions["--phase"].choices.append("command_version")
    parser._option_string_actions["--phase"].choices.append("command_interrupt")
    parser._option_string_actions["--phase"].choices.append("command_boot")
    parser._option_string_actions["--phase"].choices.append("translation_config")
    parser._option_string_actions["--phase"].choices.extend(["chat_translation", "chat_translation_stress", "chat_translation_budget"])
    parser._option_string_actions["--phase"].choices.append("dense_report")
    args = parser.parse_args()
    if args.phase == "dense_report":
        os.environ["MGLIVE_DENSE_PROOF"] = "1"
    if args.phase in ("stress", "chat_translation_stress"):
        os.environ["MGLIVE_STRESS_PREPARE"]="1"
    if args.phase.startswith("chat_translation"):
        os.environ["MGLIVE_XL_PROOF"]="1"
    if args.phase=="capture_dialog":
        assert os.environ.get("MGLIVE_FIXTURE_MG"), "Capture proof uses the existing compiler, never an app build"
        os.environ["MGLIVE_CAPTURE_PROOF"]="1"
    if args.phase == "measure":
        import measure_live
        OUT = REPO / "target/test-output/live-measure"
    OUT.mkdir(parents=True, exist_ok=True)
    if SCRATCH.is_symlink():
        raise RuntimeError("Scratch is a symlink")
    marker = SCRATCH / ".moonglow-live-events"
    if SCRATCH.exists():
        assert marker.is_file() and not marker.is_symlink() and marker.read_text().strip() == OWNER
    else:
        SCRATCH.mkdir()
        marker.write_text(OWNER + "\n")
    assert SCRATCH.resolve() == Path.home() / "mglive-events"
    for p in SCRATCH.rglob("*"):
        assert not p.is_symlink(), f"Unexpected scratch symlink: {p}"
    lock = (SCRATCH / "test.lock").open("w")
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    for protocol, port in [("-lun", GPORT), ("-ltn", RPORT)]:
        assert not subprocess.check_output(["ss", "-H", protocol, f"sport = :{port}"], text=True).strip(), f"Port {port} occupied; no process stopped"
    for name in ["modules", "logs.0", "tlk"]:
        (SCRATCH / name).mkdir(exist_ok=True)
    (OUT / "user").mkdir(exist_ok=True)
    (OUT / "app-data").mkdir(exist_ok=True)
    module = measure_live.prepare() if args.phase == "measure" else fixture()
    shutil.copyfile(module, SCRATCH / "modules/events.mod")
    if args.phase != "measure":
        shutil.copyfile(OUT/"mget_polish.tlk", SCRATCH/"tlk/mget_polish.tlk")
    codepage="cp1250" if args.phase in ["polish_cp1250", "rust_api", "mixed_speech_cp1250", "mixed_speech"] else ""
    # This task-owned scratch configuration is regenerated, never a real user's.
    (SCRATCH/"settings.tml").write_text('[game.language]\ncodepage = "'+codepage+'"\n',encoding="utf-8")
    if codepage:
        subprocess.run(["python3", str(REPO/"tools/live/wsl/configure-text.py"), str(SCRATCH/"settings.tml"), codepage],check=True)
    boot = secrets.token_hex(16)
    processes = []
    proof = None
    report = {"phase": args.phase, "boot": boot, "passed": False, "module_sha256": sha(module),
              "game_codepage":codepage,
              "player_ops_enabled": args.phase.startswith("pc_") or args.phase in ("rust_api", "teleport"),
              "compiler_sha256": sha(Path(subprocess.check_output(["wslpath", "-u", json.loads((OUT/"compile.json").read_text())["compiler_binary"]],text=True).strip())),
              "compiler_route": json.loads((OUT/"compile.json").read_text()).get("route", "guarded mg-live " + ("prepare_private_measure_fixture" if args.phase == "measure" else "prepare_private_events_fixture") + " test (single compiler worker)"),
              "server_sha256": sha(SERVER), "includes_sha256": sha(PLUGINS / "NWScript.zip"),
                "plugins": {name: sha(PLUGINS / f"NWNX_{name}.so") for name in ["Core", "Redis", "Util", "Chat", "Damage", "Events", "Creature", "Object", "Dialog"]},
              "scripts": {p.name: sha(p) for p in (REPO / "tools/live/nwn").glob("*.nss")},
              "helpers": {str(p.relative_to(REPO)): sha(p) for p in [Path(__file__),
                          REPO / "tools/live/wsl/log-tail.sh", REPO / "tools/live/wsl/log-tail.py",
                          REPO / "tools/live/wsl/owned.sh"]},
              "fixtures": {p.name: sha(p) for p in (REPO / "tools/live/fixtures/events").glob("*.nss")}}
    if args.phase == "measure":
        measured_inputs = measure_live.observations() / "inputs.json"
        report["measurement_inputs_sha256"] = sha(measured_inputs)
        report["fixtures"] = json.loads(measured_inputs.read_text())["fixture_scripts"]
        reused = os.environ.get("MGLIVE_MEASURE_REUSE", "")
        if reused:
            previous = json.loads((OUT / "runs" / reused / "measure.json").read_text())
            report["compiler_sha256"] = previous["compiler_sha256"]
            report["compiler_reused_from"] = reused
    if args.phase.startswith("chat_translation"):
        report["helpers"]["tools/live/test_chat_translation.py"] = sha(REPO / "tools/live/test_chat_translation.py")
        report["fixture_xl_sha256"] = sha(OUT / "module/mglive_xl.nss")
    if args.phase in ("translation_config", "chat_translation_budget"):
        helper = "test_translation_config.py" if args.phase == "translation_config" else "test_translation_budget.py"
        report["helpers"]["tools/live/" + helper] = sha(REPO / "tools/live" / helper)
    try:
        from stream_transport import redis_args
        redis = subprocess.Popen(redis_args(SCRATCH, RPORT),
                                 stdout=(SCRATCH / "redis.log").open("w"), stderr=subprocess.STDOUT)
        processes.append((redis, identity(redis.pid)))
        for _ in range(50):
            try:
                proof = Proof()
                break
            except ConnectionRefusedError:
                time.sleep(.1)
        assert proof
        env = {"HOME": str(Path.home()), "PATH": "/usr/bin:/bin", "LANG": "C.UTF-8",
               "LD_PRELOAD": str(PLUGINS / "NWNX_Core.so"), "NWNX_CORE_LOAD_PATH": str(PLUGINS),
               "NWNX_CORE_SKIP_ALL": "y", "NWNX_REDIS_SKIP": "n", "NWNX_UTIL_SKIP": "n", "NWNX_CHAT_SKIP": "n",
               "NWNX_DAMAGE_SKIP": "n", "NWNX_EVENTS_SKIP": "n", "NWNX_CREATURE_SKIP": "n", "NWNX_AREA_SKIP": "n", "NWNX_OBJECT_SKIP": "n", "NWNX_DIALOG_SKIP": "n",
               "NWNX_CORE_LOG_LEVEL": "6",
               "NWNX_REDIS_HOST": "127.0.0.1", "NWNX_REDIS_PORT": str(RPORT),
               "NWNX_REDIS_PUBSUB_SCRIPT": "mglive_ps", "NWNX_REDIS_PUBSUB_CHANNELS": PREFIX + ".cmd," + PREFIX + ".xl",
               "MGLIVE_PREFIX": PREFIX, "MGLIVE_BOOT": boot, "MGLIVE_AREAS": "events", "MGLIVE_ALLOW_CHUNK": "1"}
        env["MGLIVE_ALLOW_PLAYER_OPS"] = "1" if report["player_ops_enabled"] else "0"
        if args.phase == "watch_areas":
            env["MGLIVE_AREAS"] = ""
        if args.phase == "measure":
            env["MGLIVE_AREAS"] = os.environ.get("MGLIVE_DEMO_AREA", "demo_area")
        if args.phase in ("stress", "chat_translation_stress"):
            env["MGLIVE_AREAS"] = "events,events2,events3,events4,events5"
        native_started=time.time()
        server = subprocess.Popen([str(SERVER), "-userdirectory", str(SCRATCH), "-module", "events",
                                   "-port", str(GPORT), "-publicserver", "0", "-adminpassword", secrets.token_hex(24),
                                   "-servervault", "1"], cwd=SERVER.parent, env=env, stdin=subprocess.PIPE,
                                  stdout=(SCRATCH / "server.out").open("w"), stderr=subprocess.STDOUT)
        server.mglive_started=native_started
        processes.append((server, identity(server.pid)))
        # Record only identity, never argv/environment (contains the admin password).
        dump(SCRATCH / "owned-processes.json", [i for _, i in processes])
        proof.until(lambda: proof.state is not None, timeout=90)
        assert proof.state["boot"].startswith(boot + "-")  # (the launcher's, then the load's)
        if args.phase in ["log", "module_polish_repro", "module_polish", "mixed_speech"]:
            tail_env = dict(os.environ, MGLIVE_INSTANCE="events", MGLIVE_REDIS_PORT=str(RPORT),
                            MGLIVE_PREFIX=PREFIX, MGLIVE_BOOT=proof.state["boot"])
            tail = subprocess.Popen(["bash", str(REPO / "tools/live/wsl/log-tail.sh")], env=tail_env,
                                    stdout=subprocess.PIPE, stderr=(SCRATCH / "log-tail.stderr").open("w"))
            # Wait for exec into Python before recording its executable identity.
            try:
                assert select.select([tail.stdout], [], [], 5)[0] and tail.stdout.readline() == b"log-tail ready\n"
            except BaseException:
                # This direct child may still be in its shell-to-Python exec transition.
                # Reap it here even if readiness failed before identity registration.
                if tail.poll() is None:
                    tail.terminate()
                tail.wait(timeout=8)
                raise
            processes.append((tail, identity(tail.pid)))
        if args.phase == "stress":
            import stress_run
            report["checks"] = stress_run.run(proof,redis,server,processes,identity,SCRATCH)
        elif args.phase == "redis_restart":
            from test_redis_restart import run
            report["checks"] = run(proof, redis, server, processes, identity, SCRATCH)
        else:
            report["checks"] = measure_live.run(proof, server.pid) if args.phase == "measure" else getattr(proof, args.phase)()
        report["passed"] = True
        print(f"{args.phase}: engine checks passed", flush=True)
    except Exception as exc:
        report["error"] = str(exc)
        raise
    finally:
        if proof:
            if args.phase != "measure":
                dump(OUT / (args.phase + "-state.json"), proof.frames)
            if proof.logs or args.phase not in ("rust_api", "measure"):
                dump(OUT / (args.phase + "-log.json"), proof.logs)
            proof.pub.close()
            proof.sub.close()
        report["cleanup"] = []
        for process, owned in reversed(processes):
            if process.poll() is None:
                assert identity(process.pid) == owned, "Process identity changed"
                if process.stdin:
                    process.stdin.write(b"exit\n")
                    process.stdin.flush()
                else:
                    process.terminate()
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    assert identity(process.pid) == owned
                    process.terminate()
                    process.wait(timeout=8)
            report["cleanup"].append({**owned, "exited": process.poll() is not None})
        destination = measure_live.observations() if args.phase == "measure" else OUT
        if args.phase == "measure" and report.get("passed"):
            report["checks"] = {"summary": "engine.json", "sha256": sha(destination / "engine.json")}
        dump(destination / (args.phase + ".json"), report)
        if args.phase == "log" and report["passed"]:
            cleanup_rotation(report)
            dump(OUT / (args.phase + ".json"), report)


if __name__ == "__main__":
    main()
