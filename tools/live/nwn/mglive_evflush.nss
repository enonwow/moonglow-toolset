// One bounded, retry-safe XADD batch in a fresh VM entry.
#include "mglive_evt"
#include "mglive_stream"
void main()
{
    object m = GetModule();
    json events = MgLivePendingEvents();
    int count = JsonGetLength(events);
    if (!count) { DeleteLocalInt(m,"MGLIVE_EVENT_FLUSH"); return; }
    string prefix = GetLocalString(m,"MGLIVE_PREFIX") + ".events";
    string boot = GetLocalString(m,"MGLIVE_BOOT");
    // Resolve NWNX-backed helpers BEFORE pushing: another NWNXCall clears the
    // shared argument stack (otherwise Redis sees cp1250 as the command).
    string codepage = MgLiveTextCodepage();
    NWNXPushString("EVAL"); NWNXPushString(MGLIVE_STREAM_LUA); NWNXPushString("3");
    NWNXPushString(prefix); NWNXPushString(prefix+".meta");
    NWNXPushString(prefix+".producer:game:"+boot);
    NWNXPushString(boot); NWNXPushString("game"); NWNXPushString(codepage);
    int i;
    for (i=0;i<count;i++)
    {
        json e=JsonArrayGet(events,i);
        NWNXPushString(IntToString(JsonGetInt(JsonObjectGet(e,"seq"))));
        NWNXPushString(JsonDump(e));
    }
    NWNXCall("NWNX_Redis","Deferred");
    int result=NWNXPopInt();
    int stored = NWNX_Redis_GetResultType(result)==NWNX_REDIS_RESULT_INTEGER &&
        NWNX_Redis_GetResultAsInt(result)>=JsonGetInt(JsonObjectGet(JsonArrayGet(events,count-1),"seq"));
    if(stored) MgLiveClearEvents(events);
    if(GetLocalInt(m,"MGLIVE_EVENT_COUNT"))
    {
        float delay=1.0;if(stored) delay=0.0;
        DelayCommand(delay,ExecuteScript("mglive_evflush",m));
    }
    else DeleteLocalInt(m,"MGLIVE_EVENT_FLUSH");
}
