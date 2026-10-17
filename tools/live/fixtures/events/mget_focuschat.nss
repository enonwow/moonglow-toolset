#include "nwnx_chat"
void main()
{
    // Exercise callback routes without pretending an NPC is a connected PC.
    NWNX_Chat_RegisterChatScript("mget_chatcb");
    int i;
    for (i=1; i<=30; i++)
        if (i<=6 || i==14 || i==17 || i==18 || i==19 || i==20 || i==22 || i==30)
            NWNX_Chat_SendMessage(i,"focus-channel-"+IntToString(i),OBJECT_SELF);
    NWNX_Chat_RegisterChatScript("mglive_chat");
    object recipient=GetLocalObject(GetModule(),"MGET_FOCUS_LISTENER");
    SetLocalInt(GetModule(),"MGET_FOCUS_NONPC_TELL",NWNX_Chat_SendMessage(4,"not-a-player-tell",OBJECT_SELF,recipient));
    SetLocalInt(GetModule(),"MGET_FOCUS_NONPC_DM",NWNX_Chat_SendMessage(20,"not-a-player-dm-tell",OBJECT_SELF,recipient));
    SpeakString("focus-native-talk",TALKVOLUME_TALK);
    SpeakString("focus-native-whisper",TALKVOLUME_WHISPER);
}
