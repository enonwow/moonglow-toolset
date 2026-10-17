// Requires mglive_inc and mglive_result. Delivery is separate from the PC gate
// so the headless proof can exercise the exact selector with an NPC stand-in.
#include "nwnx_chat"

json MgLiveReplyDeliver(string id, object player, json requested, string text)
{
    int dm = GetLocalInt(player,"MGLIVE_CHAT_TO_DM");
    object speaker = OBJECT_INVALID;
    int channel = NWNX_CHAT_CHANNEL_PLAYER_TALK;
    if (dm)
    {
        channel = NWNX_CHAT_CHANNEL_DM_DM;
        // A connected DM supplies a native avatar identity when available.
        speaker = GetFirstPC();
        while (GetIsObjectValid(speaker) && !GetIsDM(speaker)) speaker=GetNextPC();
        if (!GetIsObjectValid(speaker)) speaker=GetModule();
    }
    else
    {
        object area=GetArea(player);
        if (JsonGetType(requested)!=JSON_TYPE_NULL)
        {
            speaker=MgLiveFindObject(JsonGetString(requested));
            if (!GetIsObjectValid(speaker)) return MgLiveResult(id,"speaker no longer exists",OBJECT_INVALID,"not_found");
            if (GetObjectType(speaker)!=OBJECT_TYPE_CREATURE) return MgLiveResult(id,"speaker must be a creature");
            if (GetArea(speaker)!=area) return MgLiveResult(id,"speaker is in another area",OBJECT_INVALID,"other_area");
            if (MgLiveProtected(speaker)) return MgLiveResult(id,"speaker must be an unprotected NPC",OBJECT_INVALID,"protected");
            if (GetIsDead(speaker)) return MgLiveResult(id,"speaker is dead",OBJECT_INVALID,"dead");
            if (GetDistanceBetween(player,speaker)>20.0) return MgLiveResult(id,"speaker must be within 20 metres");
        }
        else
        {
            float nearest=20.001;
            object candidate=GetFirstObjectInArea(area);
            while (GetIsObjectValid(candidate))
            {
                if (candidate!=player && GetObjectType(candidate)==OBJECT_TYPE_CREATURE &&
                    !MgLiveProtected(candidate) && !GetIsDead(candidate))
                {
                    float distance=GetDistanceBetween(player,candidate);
                    if (distance<=20.0 && distance<nearest) { nearest=distance; speaker=candidate; }
                }
                candidate=GetNextObjectInArea(area);
            }
            if (!GetIsObjectValid(speaker)) return MgLiveResult(id,"no eligible NPC within 20 metres",OBJECT_INVALID,"not_found");
        }
    }
    object recipient=OBJECT_INVALID;
    if (dm) recipient=player;
    // With a target, channel30 calls SendServerToPlayerChat_DM_Silent_Shout
    // directly (NWNX Chat.cpp). Talk broadcasts from the chosen NPC as usual.
    int sent=NWNX_Chat_SendMessage(channel,text,speaker,recipient);
    json value=JsonObjectSet(JsonObject(),"channel",JsonInt(channel));
    value=JsonObjectSet(value,"route",JsonString("talk"));
    if (dm) value=JsonObjectSet(value,"route",JsonString("dm"));
    value=JsonObjectSet(value,"speaker",JsonString(ObjectToString(speaker)));
    string name=GetName(speaker); if (speaker==GetModule()) name="DM";
    value=MgLiveTextField(value,"name",name);
    value=MgLiveTextField(value,"text",text);
    value=JsonObjectSet(value,"recipient",JsonString(ObjectToString(player)));
    value=JsonObjectSet(value,"submitted",JsonBool(sent));
    value=JsonObjectSet(value,"delivery_verified",JsonBool(FALSE));
    value=JsonObjectSet(value,"native",JsonString("NWNX_Chat_SendMessage"));
    string error=""; if (!sent) error="native chat submission failed (recipient must have a client)";
    json result=JsonObjectSet(MgLiveResult(id,error),"value",value);
    return JsonObjectSet(result,"text_kind",JsonString("reply"));
}

json MgLiveReplyToPlayer(json j)
{
    string id=JsonGetString(JsonObjectGet(j,"id"));
    json playerId=JsonObjectGet(j,"player");
    json requested=JsonObjectGet(j,"as");
    json message=JsonObjectGet(j,"text");
    if (JsonGetType(playerId)!=JSON_TYPE_STRING || !MgLiveObjectId(JsonGetString(playerId)) ||
        (JsonGetType(requested)!=JSON_TYPE_NULL && (JsonGetType(requested)!=JSON_TYPE_STRING || !MgLiveObjectId(JsonGetString(requested)))) ||
        JsonGetType(message)!=JSON_TYPE_STRING || !MgLiveText(JsonGetString(message)))
        return MgLiveResult(id,"player/as must be object IDs; text must be 1-200 nonblank characters");
    object player=MgLiveFindObject(JsonGetString(playerId));
    if (!GetIsObjectValid(player)) return MgLiveResult(id,"player no longer exists",OBJECT_INVALID,"not_found");
    if (!GetIsPC(player) || GetIsDM(player) || GetIsDMPossessed(player))
        return MgLiveResult(id,"player must be a PC, not an NPC or DM",OBJECT_INVALID,"protected");
    if (!GetIsObjectValid(GetArea(player))) return MgLiveResult(id,"player has no area",OBJECT_INVALID,"not_found");
    return MgLiveReplyDeliver(id,player,requested,JsonGetString(message));
}
