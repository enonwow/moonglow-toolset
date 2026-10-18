#include "mglive_evt"
#include "nwnx_chat"
#include "nwnx_redis"
#include "mglive_xl"

void main()
{
    if(GetLocalInt(GetModule(),"MGLIVE_XL_DELIVERING")) return;
    object speaker = NWNX_Chat_GetSender();
    object recipient = NWNX_Chat_GetTarget();
    int channel = NWNX_Chat_GetChannel();
    json e = MgLiveEvent("chat", speaker, recipient);
    e = JsonObjectSet(e, "channel", JsonInt(channel));
    // Engine proof: silent talk=13, silent shout=14. Channel 14 also carries
    // PC-to-DM messages: those are real chat and must stay visible.
    int silent = channel == 13 || (channel == 14 && GetIsObjectValid(speaker) &&
        !GetIsPC(speaker) && !GetIsDM(speaker) && !GetIsDMPossessed(speaker));
    e = JsonObjectSet(e, "silent", JsonBool(silent));
    int toDM = !silent && (channel == 14 || channel == 30);
    e = JsonObjectSet(e, "to_dm", JsonBool(toDM));
    e = JsonObjectSet(e, "pc", JsonBool(GetIsPC(speaker)));
    e = JsonObjectSet(e, "possessed", JsonBool(GetIsDMPossessed(speaker)));
    if (!silent && (GetIsPC(speaker) || GetIsDMPossessed(speaker)))
    {
        SetLocalInt(speaker, "MGLIVE_CHAT_TO_DM", toDM);
        SetLocalInt(speaker, "MGLIVE_CHAT_SEEN", TRUE);
    }
    // target remains the common event-filter field. recipient makes its chat
    // meaning explicit; a broadcast channel has no single recipient object.
    e = JsonObjectSet(e, "recipient", JsonObjectGet(e, "target"));
    string audience = "unknown";
    if (GetIsObjectValid(recipient)) audience = "direct";
    else if (silent) audience = "ai";
    else if (channel == 4 || channel == 20) audience = "unresolved";
    else if (channel == 14 || channel == 30) audience = "dm";
    else if (channel == 6 || channel == 22) audience = "party";
    else if (channel == 1 || channel == 3 || channel == 17 || channel == 19) audience = "vicinity";
    else if (channel == 2 || channel == 5 || channel == 18) audience = "server";
    e = JsonObjectSet(e, "audience", JsonString(audience));
    e = JsonObjectSet(e, "position", JsonNull());
    if (GetIsObjectValid(speaker) && GetIsObjectValid(GetArea(speaker)))
    {
        vector p = GetPosition(speaker);
        json position = JsonObjectSet(JsonObject(), "x", JsonFloat(p.x));
        position = JsonObjectSet(position, "y", JsonFloat(p.y));
        e = JsonObjectSet(e, "position", JsonObjectSet(position, "z", JsonFloat(p.z)));
    }
    e = MgLiveTextField(e, "text", NWNX_Chat_GetMessage());
    e = MgLiveQueueEvent(e);
    if (JsonGetType(e) != JSON_TYPE_NULL && !silent)
    {
        // The next tick retains the same seq as a reconnect/fallback copy.
        object m = GetModule();
        json packet = JsonObjectSet(JsonObject(), "v", JsonInt(1));
        JsonObjectSetInplace(packet, "boot", JsonString(GetLocalString(m,"MGLIVE_BOOT")));
        JsonObjectSetInplace(packet, "event_stream", JsonBool(TRUE));
        JsonObjectSetInplace(packet, "text_codepage", JsonString(MgLiveTextCodepage()));
        JsonObjectSetInplace(packet, "events", JsonArrayInsert(JsonArray(),e));
        NWNX_Redis_PUBLISH(GetLocalString(m,"MGLIVE_PREFIX")+".event",JsonDump(packet));
    }
    if(MgXlCapture(channel,NWNX_Chat_GetMessage(),speaker,recipient)) NWNX_Chat_SkipMessage();
}
