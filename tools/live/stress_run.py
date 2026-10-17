"""Bounded private native/HTTPS session soak. Reuses the event harness ownership."""
import gzip,hashlib,json,os,re,select,shutil,subprocess,threading,time
from pathlib import Path
from stream_transport import read_events, redis_args

ROOT=Path(__file__).resolve().parents[2]

def summary(values):
    v=sorted(values)
    return {"n":len(v),"p50":v[len(v)//2],"p95":v[min(len(v)-1,int(len(v)*.95))],"max":v[-1]} if v else {"n":0}

def run(proof,redis,server,processes,identity,scratch):
    seconds=int(os.environ.get("MGLIVE_STRESS_SECONDS","1800"));label=os.environ.get("MGLIVE_STRESS_RUN","smoke")
    assert re.fullmatch(r"[a-z0-9-]{1,39}",label)
    out=ROOT/"target/test-output/live-stress"/label;out.mkdir(exist_ok=True)
    sources=list((ROOT/"tools/live/nwn").glob("*.nss"))+list((ROOT/"tools/live/fixtures/stress").glob("*.nss"))+list((ROOT/"tools/live").glob("stress_*.py"))+[ROOT/"crates/mg-live/tests/support/stress_fixture.rs",ROOT/"crates/mg-live/tests/stress_service.rs"]
    mod=ROOT/"target/test-output/live-events/events.mod"
    (out/"manifest.json").write_text(json.dumps({"module":str(mod),"module_sha256":hashlib.sha256(mod.read_bytes()).hexdigest(),"haks":[],"commit":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),"sources":{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},"requested_seconds":seconds,"surrogates_not_connected_players":True},indent=2))
    counts={a["area"]:sum(len(a.get(k,[])) for k in ["cre","plc","doors","items"]) for a in proof.state["areas"]}
    assert len(counts)==5 and sum(counts.values())==1000,counts
    if seconds<30:
        start=time.monotonic()
        while time.monotonic()-start<seconds:proof.poll(.2)
        return {"seconds":time.monotonic()-start,"counts":counts}
    proof.command(op="run_script",script="st_setup")
    deadline=time.monotonic()+90
    while not proof.native('JsonInt(GetLocalInt(GetModule(),"ST_READY"))'):
        assert time.monotonic()<deadline,"Large look setup did not finish"
    looks=proof.native('GetLocalJson(GetModule(),"ST_LOOKS")')
    assert len(looks)==6 and all(x["inventory"]>=200 and x["look_equipment"]>=5 for x in looks),looks
    targets=[]
    for a in range(2):
        oid=proof.native('JsonString(ObjectToString(GetObjectByTag("ST_'+str(a)+'_20")))')
        targets.append({"area":"events" if a==0 else "events2","id":oid})
    (out/"targets.json").write_text(json.dumps(targets))
    for name in ["stop-service","stop-clients","clients.private.json","clients-ready"]:
        if (out/name).exists():(out/name).unlink()
    service_binary=max((ROOT/"target/debug/deps").glob("stress_service-*.exe"),key=lambda p:p.stat().st_mtime)
    # One owned executable copy avoids locking the shared Cargo output while
    # Astra1 runs library/UI gates. Reuse this path; never copy the target tree.
    host=out.parent/"service-host.exe"
    digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
    if not host.exists() or digest(host)!=digest(service_binary):shutil.copyfile(service_binary,host)
    service_binary=host
    manifest=json.loads((out/"manifest.json").read_text());manifest["service_sha256"]=digest(host)
    (out/"manifest.json").write_text(json.dumps(manifest,indent=2))
    env=dict(os.environ,MGLIVE_STRESS_RUN=label,NWN_ROOT="Z:/moonglow-no-native-game")
    env["WSLENV"]=env.get("WSLENV","")+":MGLIVE_STRESS_RUN:NWN_ROOT"
    service=subprocess.Popen([str(service_binary),"--exact","session_stress_service","--nocapture"],env=env,stdout=(out/"service-test.log").open("w"),stderr=subprocess.STDOUT)
    client=None;observer_stop=threading.Event();observed=[];report_bytes={"full":[],"delta":[]};seen=set();event_packets=0;raw_bytes=0;capture_truncated=False;resources=[];outage=None
    stream=gzip.open(out/"replay.jsonl.gz","wt",encoding="utf-8",compresslevel=3)
    boot=proof.state["boot"];owner=identity(server.pid)
    def capture():
        nonlocal raw_bytes,event_packets,capture_truncated
        cls=type(proof.pub)
        cursor="0-0"
        while not observer_stop.is_set():
            connection=None
            reader=None
            try:
                connection=cls();connection.send("SUBSCRIBE","mglive-events.state","mglive-events.event");connection.read();connection.read()
                reader=cls()
                while not observer_stop.is_set():
                    cursor, rows=read_events(reader,"mglive-events",cursor)
                    for _, fields in rows:
                        if fields["type"]!="game" or fields["source"]!=boot:continue
                        event=json.loads(fields["data"])
                        data={"v":1,"boot":boot,"text_codepage":fields["codepage"],"events":[event]}
                        line=json.dumps({"at_ms":int(time.time()*1000),"channel":"event","data":data},separators=(",",":"))
                        if raw_bytes+len(line)<128*1024*1024:stream.write(line+"\n");raw_bytes+=len(line)+1
                        else:capture_truncated=True
                        seen.add(event["seq"])
                    if not select.select([connection.sock],[],[],.25)[0]:continue
                    msg=connection.read()
                    if msg[0]!="message":continue
                    data=json.loads(msg[2]);channel=msg[1].rsplit(".",1)[1]
                    if data.get("boot")!=boot:continue
                    stamp=int(time.time()*1000)
                    line=json.dumps({"at_ms":stamp,"channel":channel,"data":data},separators=(",",":"))
                    if raw_bytes+len(line)<128*1024*1024:stream.write(line+"\n");raw_bytes+=len(line)+1
                    else:capture_truncated=True
                    if channel=="state":
                        report_bytes["delta" if "base_seq" in data else "full"].append(len(msg[2].encode()))
                        observed.append({"seq":data["seq"],"at_ms":stamp,"dropped":data.get("dropped",0)})
                    else:event_packets+=1
                    for event in data.get("events",[]):seen.add(event["seq"])
            except (OSError,EOFError,ValueError):observer_stop.wait(.2)
            finally:
                if connection:connection.close()
                if reader:reader.close()
    thread=threading.Thread(target=capture);thread.start()
    from stress_metrics import NativeMetrics
    metrics=NativeMetrics(scratch,out,server.mglive_started)
    try:
        deadline=time.monotonic()+45
        while not (out/"clients.private.json").exists():
            assert service.poll() is None and time.monotonic()<deadline,"Private service did not become ready"
            proof.poll(.2)
        win=lambda p:subprocess.check_output(["wslpath","-w",str(p)],text=True).strip()
        python=os.environ["MGLIVE_WINDOWS_PYTHON"]
        client=subprocess.Popen([python,win(ROOT/"tools/live/stress_clients.py"),win(out)],stdout=(out/"clients.stdout").open("w"),stderr=subprocess.STDOUT)
        # Establish a full replay baseline after observer/service subscriptions.
        proof.command(op="resync")
        deadline=time.monotonic()+45
        while not (out/"clients-ready").exists():
            assert client.poll() is None and time.monotonic()<deadline,"Agents did not become ready"
            proof.poll(.1)
        proof.chunk('SetLocalInt(GetModule(),"ST_RUNNING",1);')
        proof.command(op="run_script",script="st_drive")
        proof.command(op="run_script",script="st_frame")
        proof.until(lambda:all(any(e.get("text")=="ST:"+str(n) for e in proof.matching("chat")) for n in [1,2]),timeout=10)
        start=time.monotonic();next_sample=0
        while time.monotonic()-start<seconds:
            elapsed=time.monotonic()-start
            assert server.poll() is None and service.poll() is None and client.poll() is None,"Owned participant exited"
            if elapsed>=next_sample:
                stat=Path(f"/proc/{server.pid}/stat").read_text().rsplit(")",1)[1].split();status=Path(f"/proc/{server.pid}/status").read_text()
                resources.append({"seconds":elapsed,"cpu_ticks":int(stat[11])+int(stat[12]),"rss_kb":int(re.search(r"VmRSS:\s+(\d+)",status)[1])})
                (out/"progress.json").write_text(json.dumps({"seconds":elapsed,"reports":len(observed),"events":len(seen),"event_packets":event_packets,"replay_raw_bytes":raw_bytes,"latest_seq":proof.state["seq"]}))
                metrics.poll()
                next_sample+=10
            if outage is None and elapsed>=seconds/2:
                before=proof.state["seq"];proof.pub.close();proof.sub.close();redis.terminate();redis.wait(timeout=8);time.sleep(10)
                restarted=time.monotonic()
                redis=subprocess.Popen(redis_args(scratch),stdout=(scratch/"redis.log").open("a"),stderr=subprocess.STDOUT)
                processes.append((redis,identity(redis.pid)))
                while True:
                    try:proof.pub,proof.sub=type(proof.pub)(),type(proof.sub)();break
                    except ConnectionRefusedError:time.sleep(.05)
                proof.sub.send("SUBSCRIBE","mglive-events.state","mglive-events.log");proof.sub.read();proof.sub.read()
                proof.until(lambda:proof.state["seq"]>before,timeout=45)
                assert identity(server.pid)==owner and proof.state["boot"]==boot
                outage={"at_seconds":elapsed,"outage_seconds":10,"state_recovery_ms":(time.monotonic()-restarted)*1000,"same_game":True}
            proof.poll(.1)
        timed_seconds=time.monotonic()-start
        proof.chunk('SetLocalInt(GetModule(),"ST_RUNNING",0);')
        visual_tail=os.environ.get("MGLIVE_STRESS_VISUAL_TAIL")=="1"
        if visual_tail:
            # Separate, labelled post-soak native VFX rows for the GUI replay.
            proof.chunk('ApplyEffectToObject(DURATION_TYPE_PERMANENT,EffectVisualEffect(7),GetObjectByTag("ST_0_10"));ApplyEffectToObject(DURATION_TYPE_PERMANENT,EffectVisualEffect(411),GetObjectByTag("ST_0_10"));ApplyEffectToObject(DURATION_TYPE_PERMANENT,EffectVisualEffect(412),GetObjectByTag("ST_1_10"));')
            tail_end=time.monotonic()+3
            while time.monotonic()<tail_end:proof.poll(.1)
        generated=proof.native('JsonObjectSet(JsonObjectSet(JsonObjectSet(JsonObject(),"speech",JsonInt(GetLocalInt(GetModule(),"ST_SPEECH"))),"damage",JsonInt(GetLocalInt(GetModule(),"ST_DAMAGE"))),"waves",JsonInt(GetLocalInt(GetModule(),"ST_WAVE")))')
        (out/"stop-clients").touch();client.wait(timeout=45)
        if os.environ.get("MGLIVE_STREAM_PROOF")=="1":
            # Define a closed delivery interval while autonomous game AI keeps
            # running. Later events remain queued; they are not shutdown loss.
            barrier=proof.native('JsonInt(GetLocalInt(GetModule(),"MGLIVE_EVENT_SEQ"))')
            deadline=time.monotonic()+30
            delivered=0
            while delivered<barrier:
                for path in (out/"data/live/event-inbox").glob("*/inbox.jsonl"):
                    with path.open(encoding="utf-8") as source:
                        for line in source:
                            try:row=json.loads(line)
                            except json.JSONDecodeError:continue
                            if row.get("source")==boot and row.get("kind")=="game":
                                delivered=max(delivered,row["data"]["seq"])
                assert time.monotonic()<deadline,"Store did not commit the final event barrier"
                if delivered<barrier:proof.poll(.1)
            (out/"delivery-barrier.json").write_text(json.dumps({"boot":boot,"game_seq":barrier,"committed_game_seq":delivered,"meaning":"All produced game events through this inclusive sequence must be present; later live AI events remain queued."},indent=2))
        (out/"stop-service").touch();service.wait(timeout=15)
    finally:
        if client and client.poll() is None:(out/"stop-clients").touch();client.wait(timeout=45)
        if service.poll() is None:(out/"stop-service").touch();service.wait(timeout=15)
        observer_stop.set();thread.join(timeout=5);stream.close()
        native=metrics.close()
        (out/"native-metrics.json").write_text(json.dumps(native,indent=2))
    result={"seconds":timed_seconds,"total_seconds_including_drain":time.monotonic()-start,"visual_tail":visual_tail,"counts":counts,"looks":looks,"report_bytes":{k:summary(v) for k,v in report_bytes.items()},"resources":resources,"cpu_ticks_per_second":os.sysconf("SC_CLK_TCK"),"outage":outage,"generated":generated,"unique_game_events":len(seen),"missing_game_sequences":max(seen)-min(seen)+1-len(seen) if seen else 0,"event_packets":event_packets,"reported_dropped_max":max((r["dropped"] for r in observed),default=0),"capture_truncated":capture_truncated,"replay_raw_bytes":raw_bytes,"replay_compressed_bytes":(out/"replay.jsonl.gz").stat().st_size}
    result.update(native)
    (out/"native.json").write_text(json.dumps(result,indent=2));assert not native["errors"],native["errors"][:5]
    return result
