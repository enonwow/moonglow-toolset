"""Incremental native timing collector; follows all four NWN log slots across reuse."""
import gzip
import re


def distribution(values):
    values=sorted(values)
    return {"n":len(values),"p50":values[len(values)//2],"p95":values[min(len(values)-1,len(values)*95//100)],"max":values[-1]} if values else {"n":0}


class NativeMetrics:
    def __init__(self,scratch,out,started):
        self.scratch=scratch;self.started=started;self.files={};self.budgets={};self.ticks=[];self.frames=[];self.errors=[];self.error_count=0;self.diagnostics={};self.diagnostic_examples={}
        self.trace=gzip.open(out/"timings.log.gz","wt",encoding="utf-8",compresslevel=3)
        self.poll()

    def poll(self):
        for path in [*sorted((self.scratch/"logs.0").glob("nw*Log*.txt")),self.scratch/"server.out"]:
            if not path.exists():continue
            stat=path.stat()
            if stat.st_mtime<self.started:continue
            assert stat.st_size<64*1024*1024,"Native log exceeds budget"
            with path.open("rb") as source:
                first=source.readline()
                old,offset,pending=self.files.get(path,(b"",0,b""))
                if first!=old or stat.st_size<offset:offset=0;pending=b""
                source.seek(offset);data=pending+source.read();offset=source.tell()
            lines=data.split(b"\n");self.files[path]=(first,offset,lines.pop())
            for raw in lines:
                text=raw.decode("utf-8",errors="replace").rstrip("\r")
                if path.name=="server.out":self.diagnostic(text)
                else:self.line(text)

    def diagnostic(self,line):
        if not line.startswith("E "):return
        category="redis" if "NWNX_Redis" in line else "other"
        self.diagnostics[category]=self.diagnostics.get(category,0)+1
        if re.search(r"password|secret|token",line,re.I):line="[credential-related diagnostic redacted]"
        examples=self.diagnostic_examples.setdefault(category,[])
        # Strip the timestamp so repeated outage errors occupy one example.
        line=re.sub(r"^E \[[^]]+\] ","",line)[:300]
        if line not in examples and len(examples)<5:examples.append(line)

    def line(self,line):
        if "TOO MANY" in line or "Script error:" in line or "ST_SETUP_FAILED" in line:
            self.error_count+=1
            if len(self.errors)<100:self.errors.append(line)
        if "ST_VM " in line or "ST_FRAME " in line or "MGLIVE: seq=" in line:
            self.trace.write(line+"\n")
        m=re.search(r"ST_FRAME us=(\d+)",line)
        if m:self.frames.append(int(m[1]))
        m=re.search(r"ST_VM (\w+) b=(\d+) a=(\d+) us=(\d+)",line)
        if m:
            name,b,a,us=m.groups();row=self.budgets.setdefault(name,{"calls":0,"max_instructions":0,"max_us":0,"total_us":0})
            row["calls"]+=1;row["max_instructions"]=max(row["max_instructions"],1048576-int(a));row["max_us"]=max(row["max_us"],int(us));row["total_us"]+=int(us)
        m=re.search(r"MGLIVE: seq=\d+ full=\d+ tick_us=(\d+)",line)
        if m:self.ticks.append(int(m[1]))

    def close(self):
        self.poll();self.trace.close()
        return {"tick_us":distribution(self.ticks),"frame_gap_us":distribution(self.frames),"budgets":self.budgets,"errors":self.errors,"native_error_count":self.error_count,"native_log_collection":"incremental across all rotating log slots","plugin_errors":self.diagnostics,"plugin_error_examples":self.diagnostic_examples}
