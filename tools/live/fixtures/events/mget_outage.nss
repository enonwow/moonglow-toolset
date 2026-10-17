#include "nwnx_chat"
void SendOutageSpeech()
{
    NWNX_Chat_SendMessage(1, GetLocalString(OBJECT_SELF, "MGET_OUTAGE_TEXT"), OBJECT_SELF);
}
void main()
{
    // Native callback context; ExecuteScriptChunk speech has a separate known
    // empty-hook-arguments limitation and is not a Redis transport probe.
    DelayCommand(3.0, SendOutageSpeech());
}
