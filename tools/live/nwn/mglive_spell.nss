#include "mglive_evt"
#include "nwnx_events"
void main()
{
    object target = StringToObject(NWNX_Events_GetEventData("TARGET_OBJECT_ID"));
    json e = MgLiveEvent("spell", OBJECT_SELF, target);
    e = JsonObjectSet(e, "spell", JsonInt(StringToInt(NWNX_Events_GetEventData("SPELL_ID"))));
    json p = JsonObjectSet(JsonObject(), "x", JsonFloat(StringToFloat(NWNX_Events_GetEventData("TARGET_POSITION_X"))));
    p = JsonObjectSet(p, "y", JsonFloat(StringToFloat(NWNX_Events_GetEventData("TARGET_POSITION_Y"))));
    p = JsonObjectSet(p, "z", JsonFloat(StringToFloat(NWNX_Events_GetEventData("TARGET_POSITION_Z"))));
    // Object-targeted native casts can supply a zero vector in event data.
    if (GetIsObjectValid(target))
    {
        vector v = GetPosition(target);
        p = JsonObjectSet(p, "x", JsonFloat(v.x));
        p = JsonObjectSet(p, "y", JsonFloat(v.y));
        p = JsonObjectSet(p, "z", JsonFloat(v.z));
    }
    e = JsonObjectSet(e, "position", p);
    e = JsonObjectSet(e, "countered", JsonBool(StringToInt(NWNX_Events_GetEventData("SPELL_COUNTERED"))));
    e = JsonObjectSet(e, "instant", JsonBool(StringToInt(NWNX_Events_GetEventData("IS_INSTANT_SPELL"))));
    e = JsonObjectSet(e, "projectile_path", JsonInt(StringToInt(NWNX_Events_GetEventData("PROJECTILE_PATH_TYPE"))));
    MgLiveQueueEvent(e);
}
