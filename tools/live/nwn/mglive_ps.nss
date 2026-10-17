#include "mglive_exec"
#include "nwnx_redis_ps"

void main()
{
    object m = GetModule();
    if (!GetLocalInt(m, "MGLIVE_STARTED")) return;
    struct NWNX_Redis_PubSubMessageData d = NWNX_Redis_GetPubSubMessageData();
    if (d.channel != GetLocalString(m, "MGLIVE_PREFIX") + ".cmd") return;
    MgLiveHandleReply(d.message);
}
