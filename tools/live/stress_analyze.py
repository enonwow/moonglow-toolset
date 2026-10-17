"""Summarize one closed bounded native stress capture without expanding it on disk."""
import collections
import gzip
import hashlib
import json
from pathlib import Path
import sys


def distribution(rows):
    rows=sorted(rows)
    return {"n":len(rows),"p50":rows[len(rows)//2],"p95":rows[min(len(rows)-1,len(rows)*95//100)],"max":rows[-1]} if rows else {"n":0}


def analyze(folder):
    native=json.loads((folder/"native.json").read_text())
    clients=json.loads((folder/"clients.json").read_text())
    seen=set(); speech=set(); window_seen=set(); window_start=None; window_dropped=0; window_reports=0; reports=[]; full_times=[]; looks=collections.Counter(); kinds=collections.Counter(); states={}; moved=set(); max_events=0
    samples=[]; started=False; boot=None; first_ms=None; last_ms=0; arrivals={}
    with gzip.open(folder/"replay.jsonl.gz","rt",encoding="utf-8") as stream:
        for line in stream:
            packet=json.loads(line); data=packet["data"]; boot=data.get("boot",boot)
            first_ms=packet["at_ms"] if first_ms is None else first_ms;last_ms=packet["at_ms"]
            if window_start is None and any(e.get("text", "").startswith("ST:") for e in data.get("events", [])):
                window_start=packet["at_ms"]
            in_window=window_start is not None and window_start<=packet["at_ms"]<window_start+300000
            if in_window:
                window_seen.update(e["seq"] for e in data.get("events", []))
                if packet["channel"]=="state":
                    window_reports+=1
                    window_dropped=max(window_dropped,data.get("dropped",0))
            if packet["channel"]=="state":
                reports.append((packet["at_ms"],data["seq"]))
                max_events=max(max_events,len(data.get("events",[])))
                if "base_seq" not in data:started=True;full_times.append(packet["at_ms"])
                if started and len(samples)<100:samples.append(data)
                for area in data["areas"]:
                    for row in area.get("cre",[]):
                        key=(area["area"],row["id"]);pos=(row.get("x"),row.get("y"))
                        if key in states and states[key]!=pos:moved.add(key)
                        states[key]=pos
                        if "look" in row:looks["rows"]+=1;looks["bytes"]+=len(json.dumps(row["look"],separators=(",",":")))
            for event in data.get("events",[]):
                seq=event["seq"]
                if seq not in seen:
                    seen.add(seq);kinds[event.get("kind","unknown")]+=1;arrivals[seq]=(packet["at_ms"],event.get("kind","unknown"))
                    if event.get("kind")=="chat" and event.get("text","").startswith("ST:"):speech.add(event["text"])
    conversations=kinds["dialog"]
    # Use all later packets to distinguish losses from the window's pending tail.
    if window_seen:
        missing=sum(seq not in seen for seq in range(min(window_seen),max(window_seen)+1))
        window={"start_ms":window_start,"seconds":300,"events_arrived_within_window":len(window_seen),"missing_after_drain":missing,"dropped_max":window_dropped,"reports":window_reports,"limits":"starts at first ST speech; full capture used to exclude pending tail; outage timing must be compared separately"}
        (folder/"first-300.json").write_text(json.dumps(window,indent=2))
    gaps=[b[0]-a[0] for a,b in zip(reports,reports[1:])]
    def process(rows,key,scale=1):
        if len(rows)<2:return {}
        first,last=rows[0],rows[-1]; seconds=last["seconds"]-first["seconds"]
        return {"seconds":seconds,"one_core_percent":100*(last[key]-first[key])*scale/seconds}
    memory=clients["resources"]
    result={"native_seconds":native["seconds"],"report_interarrival_ms":distribution(gaps),"contiguous_report_interarrival_ms":distribution([b[0]-a[0] for a,b in zip(reports,reports[1:]) if b[1]==a[1]+1]),"full_interarrival_ms":distribution([b-a for a,b in zip(full_times,full_times[1:])]),"event_kinds":dict(kinds),"unique_speech":len(speech),"generated_speech":native["generated"]["speech"],"dialog_lines":conversations,"max_events_in_report":max_events,"moving_creatures":len(moved),"look_wire":dict(looks),"native_cpu":process(native["resources"],"cpu_ticks",1/native.get("cpu_ticks_per_second",100)),"service_cpu":process(memory,"cpu_s"),"service_memory_bytes":{"first_private":memory[0]["private"],"last_private":memory[-1]["private"],"max_private":max(r["private"] for r in memory),"after_five_minutes_private":next((r["private"] for r in memory if r["seconds"]>=300),None)},"capture_truncated":native.get("capture_truncated",False),"sample_reports":len(samples)}
    # Compare the independent native observer with real store/log-writer output.
    # Ignore older boot folders when this single workset has been reused.
    logs=folder/"data/live/logs"/"".join(c for c in (boot or "") if c.isascii() and c.isalnum())[:64]
    logged=collections.Counter(); log_sources={}
    for path in logs.glob("*.jsonl"):
        log_sources[path.name]=hashlib.sha256(path.read_bytes()).hexdigest()
        with path.open(encoding="utf-8") as stream:
            for line in stream:
                row=json.loads(line);seq=row.get("data",{}).get("game_seq")
                if seq is not None:logged[seq]+=1
    receipt=folder/"store-native-seqs.json.gz"
    if logged:
        with gzip.open(receipt,"wt",encoding="utf-8",compresslevel=6) as stream:
            json.dump({"boot":boot,"source_sha256":log_sources,"rows":sorted(logged.items())},stream,separators=(",",":"))
    elif receipt.exists():
        with gzip.open(receipt,"rt",encoding="utf-8") as stream:value=json.load(stream)
        assert value["boot"]==boot,"Store-log receipt belongs to a different native boot"
        logged=collections.Counter(dict(value["rows"]))
    if logged:
        last=max(logged);missing=seen-set(logged)
        interior=[s for s in missing if arrivals[s][0]<last_ms-2000]
        result["store_log_delivery"]={"unlogged_before_final_two_seconds":len(interior),"unlogged_final_two_seconds":len(missing)-len(interior),"interior_kinds":dict(collections.Counter(arrivals[s][1] for s in interior)),"interior_capture_seconds_buckets":dict(sorted(collections.Counter((arrivals[s][0]-first_ms)//10000*10 for s in interior).items())),"native_boot":boot,"unique":len(logged),"duplicate_rows":sum(n-1 for n in logged.values()),"missing_within_logged_sequence_span":len([s for s in missing if min(logged)<=s<=last]),"unflushed_capture_tail":len([s for s in missing if s>last]),"capture_before_logger":len([s for s in missing if s<min(logged)]),"scope":"current boot only; native IDs across the complete capture; durable runs additionally use an explicit final barrier"}
        boundary=folder/"delivery-barrier.json"
        if boundary.exists():
            barrier=json.loads(boundary.read_text());assert barrier["boot"]==boot
            upper=barrier["game_seq"]
            required=set(range(1,upper+1))
            result["durable_delivery"]={"produced_through_barrier":upper,"logged":len(required&set(logged)),"lost":len(required-set(logged)),"duplicates":sum(max(0,logged[n]-1) for n in required),"observer_missing":len(required-seen),"committed_through":barrier["committed_game_seq"],"scope":"Complete game boot through the explicit inclusive native sequence barrier; post-barrier AI events remain queued."}
    assert max_events<=256, max_events
    (folder/"analysis.json").write_text(json.dumps(result,indent=2))
    # One small current replay workset for the Rust parse/merge microbenchmark.
    (folder/"sample-reports.json").write_text(json.dumps(samples,separators=(",",":")))
    print(json.dumps(result,indent=2))

if __name__=="__main__":analyze(Path(sys.argv[1]))
