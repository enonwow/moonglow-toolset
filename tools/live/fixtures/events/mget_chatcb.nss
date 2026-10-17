#include "nwnx_chat"
void main()
{
    ExecuteScript("mglive_chat", OBJECT_SELF);
    // Exercise every collector channel without pretending an NPC is a player.
    // Native delivery of party/tell channels requires a real connected client.
    NWNX_Chat_SkipMessage();
}
