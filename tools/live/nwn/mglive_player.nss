#include "mglive_evt"
#include "nwnx_events"
void main()
{
    string current = NWNX_Events_GetCurrentEvent();
    string player = NWNX_Events_GetEventData("PLAYER_NAME");
    string key = "MGLIVE_PLAYER_" + player;
    object m = GetModule();
    if (current == NWNX_ON_CLIENT_DISCONNECT_BEFORE)
    {
        // AFTER may run after the character has been removed from the world.
        SetLocalJson(m, key, MgLiveEvent("player_leave", OBJECT_SELF));
        return;
    }
    json e;
    if (current == NWNX_ON_CLIENT_CONNECT_AFTER)
    {
        e = MgLiveEvent("player_join");
        e = JsonObjectSet(e, "dm", JsonBool(StringToInt(NWNX_Events_GetEventData("IS_DM"))));
        // This native hook runs at character selection; no character exists yet.
    }
    else if (current == NWNX_ON_CLIENT_DISCONNECT_AFTER)
    {
        e = MgLiveEvent("player_leave", OBJECT_SELF);
        json saved = GetLocalJson(m, key);
        if (JsonGetType(saved) == JSON_TYPE_OBJECT)
        {
            e = JsonObjectSet(e, "id", JsonObjectGet(saved, "id"));
            e = JsonObjectSet(e, "name", JsonObjectGet(saved, "name"));
            e = JsonObjectSet(e, "area", JsonObjectGet(saved, "area"));
        }
        DeleteLocalJson(m, key);
    }
    else return;
    e = JsonObjectSet(e, "player", JsonString(player));
    MgLiveQueueEvent(e);
}
