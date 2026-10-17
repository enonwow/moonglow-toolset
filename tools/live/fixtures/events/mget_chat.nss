#include "nwnx_chat"
void main()
{
    object a = GetObjectByTag("NW_BANDIT001");
    int mode = GetLocalInt(GetModule(), "MGET_CHAT_BURST");
    int i;
    if (mode)
    {
        int offset=GetLocalInt(GetModule(),"MGET_CHAT_OFFSET");
        for (i = 0; i < 300; i++) NWNX_Chat_SendMessage(1, "burst-" + IntToString(offset+i), a);
        SetLocalInt(GetModule(),"MGET_CHAT_OFFSET",offset+300);
        return;
    }
    NWNX_Chat_RegisterChatScript("mget_chatcb");
    for (i = 1; i <= 30; i++)
        if (i <= 6 || i == 14 || i == 17 || i == 18 || i == 19 || i == 20 || i == 22 || i == 30)
            NWNX_Chat_SendMessage(i, "channel-" + IntToString(i), a);
    NWNX_Chat_RegisterChatScript("mglive_chat");
}
