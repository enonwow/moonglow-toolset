#include "mglive_exec"
#include "nwnx_redis_ps"
#include "mglive_xl"

void main()
{
    object m = GetModule();
    if (!GetLocalInt(m, "MGLIVE_STARTED")) return;
    struct NWNX_Redis_PubSubMessageData d = NWNX_Redis_GetPubSubMessageData();
    if(d.channel==GetLocalString(m,"MGLIVE_PREFIX")+".xl")
    {
        MgXlReply(d.message);
        return;
    }
    if (d.channel != GetLocalString(m, "MGLIVE_PREFIX") + ".cmd") return;
    MgLiveHandleReply(d.message);
}
