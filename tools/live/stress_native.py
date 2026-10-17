"""Private stress preparation; one authored MOD workset, no demo mutations."""
import os
from pathlib import Path
import re
import shutil

ROOT = Path(__file__).resolve().parents[2]


def prepare(out, module):
    assert not os.environ.get("MGLIVE_FIXTURE_MG"), "Stress uses the guarded library fixture"
    # Explicit NPC surrogate, not a fabricated player flag.
    p=module/"mglive_inc.nss";s=p.read_text()
    needle='if (GetIsPC(o)) look = MgLiveLook(o);'
    assert needle in s
    s=s.replace(needle,'if (GetIsPC(o) || GetLocalInt(o,"ST_PC_LOOK")) look = MgLiveLook(o);')
    s=s.replace('JsonInt(GetIsPC(o) ? HashString(JsonDump(look))', 'JsonInt((GetIsPC(o) || GetLocalInt(o,"ST_PC_LOOK")) ? HashString(JsonDump(look))')
    p.write_text(s)
    for p in (ROOT/"tools/live/fixtures/stress").glob("*.nss"):
        shutil.copyfile(p,module/p.name)
    shutil.copyfile(ROOT/"tools/live/measure-full-scale.nss",module/"mp_scale.nss")
    shutil.copyfile(ROOT/"tools/live/measure-look-stress.nss",module/"mp_stress.nss")
    # Report every tick; unchanged native work, observer overhead is explicit.
    p=module/("mglive_report.nss" if (module/"mglive_report.nss").exists() else "mglive_tick.nss");s=p.read_text().replace('if (seq == 1 || seq % 60 == 0)','if (TRUE)').replace('if(seq==1 || seq%60==0)','if(TRUE)');p.write_text(s)
    for p in list(module.glob("mglive_*.nss"))+list(module.glob("st_*.nss")):
        s=p.read_text()
        if not re.search(r"(?m)^void[ \t]+main[ \t]*\([ \t]*\)",s):continue
        s=re.sub(r"(?m)^void[ \t]+main[ \t]*\([ \t]*\)","void StObservedMain()",s,count=1)
        s+='\nvoid main(){int b=GetScriptInstructionsRemaining();int t=GetMicrosecondCounter();StObservedMain();int a=GetScriptInstructionsRemaining();WriteTimestampedLogEntry("ST_VM '+p.stem+' b="+IntToString(b)+" a="+IntToString(a)+" us="+IntToString(GetMicrosecondCounter()-t));}\n'
        p.write_text(s)
