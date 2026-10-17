// Included after MgLiveResult/MgLiveCancelHold and the bounded GFF JSON helpers.
json MgLiveTeleport(json j, int allowPlayers, int playerAlias)
{
    string id = JsonGetString(JsonObjectGet(j, "id"));
    string key = JsonGetString(JsonObjectGet(j, "target"));
    if (!MgLiveObjectId(key)) return MgLiveResult(id, "invalid target id");
    object target = MgLiveFindObject(key);
    if (!GetIsObjectValid(target)) return MgLiveResult(id, "target no longer exists", OBJECT_INVALID, "not_found");
    int type = GetObjectType(target);
    if (type != OBJECT_TYPE_CREATURE && type != OBJECT_TYPE_PLACEABLE && type != OBJECT_TYPE_DOOR && type != OBJECT_TYPE_ITEM)
        return MgLiveResult(id, "target must be a creature, placeable, door or ground item");
    if (playerAlias && type != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "pc_teleport requires a creature");
    if (MgLiveProtected(target) && !allowPlayers)
        return MgLiveResult(id, "player operations disabled or players flag missing", OBJECT_INVALID, "disabled");
    if (type == OBJECT_TYPE_CREATURE && GetIsDead(target)) return MgLiveResult(id, "target is dead", OBJECT_INVALID, "dead");
    if (type == OBJECT_TYPE_ITEM && GetIsObjectValid(GetItemPossessor(target)))
        return MgLiveResult(id, "inventory items cannot be teleported", OBJECT_INVALID, "protected");
    object source = GetArea(target);
    if (!GetIsObjectValid(source)) return MgLiveResult(id, "target has no area", OBJECT_INVALID, "not_found");
    object area = source;
    json areaValue = JsonObjectGet(j, "area");
    if (JsonGetType(areaValue) != JSON_TYPE_NULL)
    {
        if (JsonGetType(areaValue) != JSON_TYPE_STRING) return MgLiveResult(id, "area must be a resref");
        string name = JsonGetString(areaValue);
        if (name != "")
        {
            if (!MgLiveResRef(name)) return MgLiveResult(id, "invalid area resref");
            area = MgLiveFindArea(name);
        }
    }
    if (!GetIsObjectValid(area)) return MgLiveResult(id, "unknown destination area", OBJECT_INVALID, "not_found");
    if (!MgLivePosition(j)) return MgLiveResult(id, "finite xyz required");
    vector at = Vector(JsonGetFloat(JsonObjectGet(j, "x")), JsonGetFloat(JsonObjectGet(j, "y")), JsonGetFloat(JsonObjectGet(j, "z")));
    if (at.x < 0.0 || at.y < 0.0 || at.x >= IntToFloat(GetAreaSize(AREA_WIDTH, area))*10.0 || at.y >= IntToFloat(GetAreaSize(AREA_HEIGHT, area))*10.0)
        return MgLiveResult(id, "xy must be inside destination area bounds");
    float facing = GetFacing(target);
    json angle = JsonObjectGet(j, "f");
    if (JsonGetType(angle) != JSON_TYPE_NULL)
    {
        if (!MgLiveNumber(angle) || JsonGetFloat(angle) < 0.0 || JsonGetFloat(angle) >= 360.0)
            return MgLiveResult(id, "f must be finite, >=0 and <360");
        facing = JsonGetFloat(angle);
    }
    object moved = target;
    int recreated = FALSE;
    if (type == OBJECT_TYPE_CREATURE)
    {
        MgLiveCancelHold(target);
        AssignCommand(target, ClearAllActions(TRUE));
        AssignCommand(target, JumpToLocation(Location(area, at, facing)));
    }
    else if (source != area || (type == OBJECT_TYPE_DOOR && fabs(facing - GetFacing(target)) > 0.01))
    {
        json snapshot = ObjectToJson(target, TRUE);
        if (!MgLiveObjectJson(snapshot)) return MgLiveResult(id, "object snapshot invalid or larger than 256 KB");
        moved = JsonToObject(snapshot, Location(area, at, facing), OBJECT_INVALID, TRUE);
        if (!GetIsObjectValid(moved)) return MgLiveResult(id, "native recreation failed; original retained", OBJECT_INVALID, "disabled");
        if (GetObjectType(moved) != type || GetArea(moved) != area)
        {
            DestroyObject(moved);
            return MgLiveResult(id, "native recreation mismatched; original retained", OBJECT_INVALID, "disabled");
        }
        SetLocalInt(moved, "MGLIVE_OWN", GetLocalInt(target, "MGLIVE_OWN"));
        DestroyObject(target);
        recreated = TRUE;
    }
    else
    {
        // Door rotations use the recreation path above: SetFacing ignores doors.
        NWNX_Object_SetPosition(target, at);
        if (type != OBJECT_TYPE_DOOR) AssignCommand(target, SetFacing(facing));
    }
    // Briefly expose both sides for verification/follow handoff. A teleport
    // must not leave an invisible permanent subscription behind.
    int until = GetLocalInt(GetModule(), "MGLIVE_SEQ") + 5;
    SetLocalInt(source, "MGLIVE_WATCH_UNTIL", until);
    SetLocalInt(area, "MGLIVE_WATCH_UNTIL", until);
    SetLocalInt(GetModule(), "MGLIVE_RESYNC", TRUE);
    json value = JsonObjectSet(JsonObject(), "previous", JsonString(key));
    value = JsonObjectSet(value, "recreated", JsonBool(recreated));
    value = JsonObjectSet(value, "area", JsonString(GetResRef(area)));
    return JsonObjectSet(MgLiveResult(id, "", moved), "value", value);
}
