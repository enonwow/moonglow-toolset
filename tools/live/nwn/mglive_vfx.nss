// Observable projectiles only. General instantaneous visual effects are deferred.
#include "mglive_evt"
#include "nwnx_events"
void main()
{
    object target = StringToObject(NWNX_Events_GetEventData("TARGET_OBJECT_ID"));
    json e = MgLiveEvent("vfx", OBJECT_SELF, target);
    e = JsonObjectSet(e, "subtype", JsonString("projectile"));
    e = JsonObjectSet(e, "spell", JsonInt(StringToInt(NWNX_Events_GetEventData("SPELL_ID"))));
    e = JsonObjectSet(e, "projectile_type", JsonInt(StringToInt(NWNX_Events_GetEventData("PROJECTILE_TYPE"))));
    e = JsonObjectSet(e, "projectile_path", JsonInt(StringToInt(NWNX_Events_GetEventData("PROJECTILE_PATH_TYPE"))));
    e = JsonObjectSet(e, "delta_ms", JsonInt(StringToInt(NWNX_Events_GetEventData("DELTA"))));
    e = JsonObjectSet(e, "attack_result", JsonInt(StringToInt(NWNX_Events_GetEventData("ATTACK_RESULT"))));
    json p = JsonObjectSet(JsonObject(), "x", JsonFloat(StringToFloat(NWNX_Events_GetEventData("TARGET_POSITION_X"))));
    p = JsonObjectSet(p, "y", JsonFloat(StringToFloat(NWNX_Events_GetEventData("TARGET_POSITION_Y"))));
    p = JsonObjectSet(p, "z", JsonFloat(StringToFloat(NWNX_Events_GetEventData("TARGET_POSITION_Z"))));
    e = JsonObjectSet(e, "position", p);
    MgLiveQueueEvent(e);
}
