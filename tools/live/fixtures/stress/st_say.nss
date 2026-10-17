#include "nwnx_chat"
#include "nwnx_redis"
void main()
{
    object m=GetModule();int n=GetLocalInt(m,"ST_SPEECH")+1;SetLocalInt(m,"ST_SPEECH",n);
    string text="ST:"+IntToString(n);
    NWNX_Redis_PUBLISH("mglive-events.marker",text);
    NWNX_Chat_SendMessage(1,text,OBJECT_SELF);
}
