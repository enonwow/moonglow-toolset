#include "mglive_inc"
#include "mglive_nui"

void main()
{
    object m = GetModule();
    if (GetLocalInt(m, "MGLIVE_STARTED")) return;
    if (NWNX_Util_GetEnvironmentVariable("MGLIVE_PREFIX") == "" &&
        NWNX_Util_GetEnvironmentVariable("NWNX_REDIS_HOST") == "") return;
    MgLiveConfigure();
    MgLiveNuiInstall();
    SetLocalInt(m, "MGLIVE_STARTED", TRUE);
    SetLocalJson(m, "MGLIVE_ACK", JsonArray());
    SetLocalJson(m, "MGLIVE_RESULTS", JsonArray());
    SetLocalJson(m, "MGLIVE_DONE", JsonArray());
    SetLocalInt(m, "MGLIVE_EVENT_HEAD", 0);
    SetLocalInt(m, "MGLIVE_EVENT_COUNT", 0);
    SetLocalInt(m, "MGLIVE_EVENT_SEQ", 0);
    SetLocalInt(m, "MGLIVE_DROPPED", 0);
    ExecuteScript("mglive_evload", m);
    WriteTimestampedLogEntry("MGLIVE: started boot=" + GetLocalString(m, "MGLIVE_BOOT") +
        " prefix=" + GetLocalString(m, "MGLIVE_PREFIX"));
    ExecuteScript("mglive_tick", m);
}
