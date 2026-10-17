#include "mglive_text"
#include "mglive_dm"
#include "nwnx_chat"
void main()
{
    SetLocalJson(OBJECT_SELF,"MGET_DM_REPLY",MgLiveDmReply(OBJECT_SELF,"Private reply proof"));
    NWNX_Chat_SendMessage(NWNX_CHAT_CHANNEL_DM_DM,"to-dm-proof",OBJECT_SELF);
    NWNX_Chat_SendMessage(NWNX_CHAT_CHANNEL_PLAYER_DM,"silent-ai-proof",OBJECT_SELF);
}
