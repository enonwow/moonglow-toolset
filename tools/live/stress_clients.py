"""Windows-side real HTTPS agents + MCP watch, compact timing and process samples."""
import ctypes
import hashlib
import http.client
import json
import os
from pathlib import Path
import socket
import ssl
import subprocess
import sys
import threading
import time


def stats(xs):
    xs=sorted(xs)
    return {"n":len(xs),"p50":xs[len(xs)//2],"p95":xs[min(len(xs)-1,int(len(xs)*.95))],"max":xs[-1]} if xs else {"n":0}


class Api:
    def __init__(self, auth, cert, token):
        context=ssl.create_default_context(cadata=ssl.DER_cert_to_PEM_cert(cert));context.check_hostname=False
        self.port=auth["port"];self.context=context;self.token=token;self.connection=None
    def call(self,op,body):
        if self.connection is None:self.connection=http.client.HTTPSConnection("127.0.0.1",self.port,context=self.context,timeout=35)
        try:
            self.connection.request("POST","/v1/call/live_"+op,json.dumps(body),{"Authorization":"Bearer "+self.token,"Content-Type":"application/json","Connection":"keep-alive"})
            reply=self.connection.getresponse();data=json.loads(reply.read())
            return reply.status,data
        except Exception:
            self.connection.close();self.connection=None;raise


def process_sample(pid):
    class Memory(ctypes.Structure):
        _fields_=[("cb",ctypes.c_ulong),("faults",ctypes.c_ulong)]+[(k,ctypes.c_size_t) for k in ["peak_working","working","peak_paged","paged","peak_nonpaged","nonpaged","pagefile","peak_pagefile","private"]]
    kernel=ctypes.windll.kernel32;kernel.OpenProcess.restype=ctypes.c_void_p
    handle=kernel.OpenProcess(0x410,False,pid)
    if not handle:return {}
    kernel.GetProcessTimes.argtypes=[ctypes.c_void_p,*([ctypes.c_void_p]*4)]
    ctypes.windll.psapi.GetProcessMemoryInfo.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_ulong]
    kernel.CloseHandle.argtypes=[ctypes.c_void_p]
    values=[ctypes.c_ulonglong() for _ in range(4)];memory=Memory();memory.cb=ctypes.sizeof(memory)
    kernel.GetProcessTimes(handle,*(ctypes.byref(v) for v in values));ctypes.windll.psapi.GetProcessMemoryInfo(handle,ctypes.byref(memory),memory.cb);kernel.CloseHandle(handle)
    return {"cpu_s":(values[2].value+values[3].value)/1e7,"private":memory.private,"working":memory.working}


def main():
    out=Path(sys.argv[1]);auth=json.loads((out/"clients.private.json").read_text());cert=(out/"certificate.der").read_bytes()
    targets=json.loads((out/"targets.json").read_text());stop=threading.Event();lock=threading.Lock()
    timings={"call":[],"command":[],"marker_https":[]};errors=[];commands={};heard=set();duplicate=0;markers={};resources=[]
    def observe():
        # Same Windows clock for native marker arrival and HTTPS/MCP return.
        while not stop.is_set():
            try:
                sock=socket.create_connection(("127.0.0.1",6392),3);sock.settimeout(2);stream=sock.makefile("rb")
                def read():
                    line=stream.readline();tag=line[:1];data=line[1:-2]
                    if not line:raise EOFError()
                    if tag==b"*":return [read() for _ in range(int(data))]
                    if tag==b"$":n=int(data);value=stream.read(n);stream.read(2);return value
                    return data
                sock.sendall(b'*2\r\n$9\r\nSUBSCRIBE\r\n$20\r\nmglive-events.marker\r\n');read()
                while not stop.is_set():
                    row=read()
                    if row[0]==b"message":
                        with lock:markers[row[2].decode()]=time.perf_counter()
            except (OSError,ValueError,EOFError):stop.wait(.2)
    def writer(client,index):
        api=Api(auth,cert,client["token"]);n=0
        while not stop.is_set():
            started=time.perf_counter();pending=[]
            for _ in range(2):
                n+=1;ident=f"stress-{index}-{n}";text=f"Agent:{index}:{n}";row=targets[index]
                called=time.perf_counter()
                try:
                    status,reply=api.call("npc_say",{"id":ident,"area":row["area"],"target":row["id"],"text":text})
                    with lock:timings["call"].append((time.perf_counter()-called)*1000)
                    if status!=200 or not reply.get("ok"):raise RuntimeError(str(reply)[:200])
                    pending.append((reply["result"]["queued"],text,called))
                except Exception as e:
                    with lock:errors.append({"client":index,"id":ident,"error":str(e)[:220]})
            for queued,text,called in pending:
                try:
                    completed=False
                    while not stop.is_set():
                        _,answer=api.call("wait",{"id":queued,"timeout_ms":5000});result=answer.get("result",{})
                        if not result.get("pending"):
                            if not result.get("ok"):raise RuntimeError(str(result)[:200])
                            completed=True
                            break
                        stop.wait(.05)
                    if not completed:continue
                    with lock:commands[text]={"id":queued,"latency_ms":(time.perf_counter()-called)*1000};timings["command"].append((time.perf_counter()-called)*1000)
                except Exception as e:
                    with lock:errors.append({"client":index,"id":queued,"error":str(e)[:220]})
            stop.wait(max(0,1-(time.perf_counter()-started)))
    control=Api(auth,cert,auth["default"]);deadline=time.monotonic()+40
    while True:
        _,state=control.call("status",{})
        if state.get("result",{}).get("seq",0)>0:break
        assert time.monotonic()<deadline,"Live service did not receive a baseline"
        time.sleep(.1)
    watch_token=auth["clients"][2]["token"]
    desc=out/"watch.private.json";desc.write_text(json.dumps({"protocol":"moonglow-agent/1","url":f'https://127.0.0.1:{auth["port"]}',"token":watch_token,"certSha256":hashlib.sha256(cert).hexdigest(),"pid":auth["pid"],"version":"stress"}))
    binary=Path(__file__).resolve().parents[2]/"target/debug/moonglow-mcp.exe"
    # A reused workset now restores durable history. Reconcile the current
    # cursor before starting any writers; cursor0 correctly reports eviction
    # when that history exceeds one API page.
    _,page=control.call("events",{"limit":1})
    cursor=page["result"]["last"]
    watcher=subprocess.Popen([str(binary),"--descriptor",str(desc),"watch","--session",str(auth["pid"]),"--since",str(cursor)],stdout=subprocess.PIPE,stderr=(out/"watch.stderr").open("w"),text=True,encoding="utf-8")
    def watch():
        nonlocal duplicate
        for line in watcher.stdout:
            try:
                e=json.loads(line);data=e.get("data",{});text=data.get("text","")
                with lock:
                    if text.startswith("Agent:"):
                        if text in heard:duplicate+=1
                        heard.add(text)
                    if text.startswith("ST:") and text in markers:timings["marker_https"].append((time.perf_counter()-markers.pop(text))*1000)
            except ValueError:pass
    threads=[threading.Thread(target=observe),threading.Thread(target=watch)]+[threading.Thread(target=writer,args=(auth["clients"][i],i)) for i in range(2)]
    for t in threads:t.start()
    (out/"clients-ready").touch()
    started=time.monotonic();next_sample=0
    try:
        while not (out/"stop-clients").exists() and time.monotonic()-started<3700:
            assert watcher.poll() is None,"MCP watch exited; the run is not an agent delivery proof"
            if time.monotonic()-started>=next_sample:
                resources.append({"seconds":time.monotonic()-started,**process_sample(auth["pid"])})
                next_sample+=10
                with lock:
                    (out/"client-progress.json").write_text(json.dumps({"seconds":time.monotonic()-started,"commands":len(commands),"heard":len(heard),"errors":len(errors),"marker_samples":len(timings["marker_https"]),"watch_exit":watcher.poll()}))
            time.sleep(.2)
    finally:
        watch_exit_before_stop=watcher.poll()
        stop.set();watcher.terminate();watcher.wait(timeout=8)
        for t in threads:t.join(timeout=38)
        desc.unlink()
    with lock:
        (out/"clients.json").write_text(json.dumps({"seconds":time.monotonic()-started,"timings_ms":{k:stats(v) for k,v in timings.items()},"command_count":len(commands),"heard":len(heard),"duplicates":duplicate,"missing":sorted(set(commands)-heard),"errors":errors,"resources":resources,"watch_exit_before_stop":watch_exit_before_stop,"mcp_sha256":hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))


if __name__=="__main__":main()
