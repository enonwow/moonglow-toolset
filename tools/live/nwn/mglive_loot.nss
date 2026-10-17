// Called after shared identity/area/player guards. Restore only flags claimed here.
int MgLiveNoDrop(object item, int disabled)
{
    if (!GetIsObjectValid(item)) return 0;
    if (disabled)
    {
        if (!GetLocalInt(item, "MGLIVE_DROP_SAVED"))
        {
            SetLocalInt(item, "MGLIVE_DROP_SAVED", TRUE);
            SetLocalInt(item, "MGLIVE_DROP_ORIGINAL", GetDroppableFlag(item));
        }
        SetDroppableFlag(item, FALSE);
        return 1;
    }
    if (!GetLocalInt(item, "MGLIVE_DROP_SAVED")) return 0;
    // A script which has since enabled drops owns its newer decision.
    if (!GetDroppableFlag(item)) SetDroppableFlag(item, GetLocalInt(item, "MGLIVE_DROP_ORIGINAL"));
    DeleteLocalInt(item, "MGLIVE_DROP_ORIGINAL");
    DeleteLocalInt(item, "MGLIVE_DROP_SAVED");
    return 1;
}
json MgLiveLoot(json j, object target, string id)
{
    if (GetObjectType(target) != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target must be a creature");
    if (GetIsPC(target) || GetIsDM(target) || GetIsDMPossessed(target)) return MgLiveResult(id, "lootable is an NPC corpse property", OBJECT_INVALID, "protected");
    if (GetIsDead(target)) return MgLiveResult(id, "set loot before death", OBJECT_INVALID, "dead");
    json loot = JsonObjectGet(j, "lootable");
    json drop = JsonObjectGet(j, "no_drop");
    if (JsonGetType(loot) != JSON_TYPE_BOOL || (JsonGetType(drop) != JSON_TYPE_NULL && JsonGetType(drop) != JSON_TYPE_BOOL)) return MgLiveResult(id, "lootable and optional no_drop must be boolean");
    json items = JsonArray();
    if (JsonGetType(drop) == JSON_TYPE_BOOL)
    {
        object item = GetFirstItemInInventory(target);
        while (GetIsObjectValid(item))
        {
            items = JsonArrayInsert(items,JsonString(ObjectToString(item)));
            if (JsonGetLength(items)>256) return MgLiveResult(id,"no_drop limited to 256 items; no changes made");
            item = GetNextItemInInventory(target);
        }
        int slot;
        for (slot = 0; slot < NUM_INVENTORY_SLOTS; slot++)
        {
            item = GetItemInSlot(slot,target);
            if (GetIsObjectValid(item) && !MgLiveContains(items,ObjectToString(item))) items = JsonArrayInsert(items,JsonString(ObjectToString(item)));
        }
        if (JsonGetLength(items)>256) return MgLiveResult(id,"no_drop limited to 256 items; no changes made");
    }
    SetLootable(target, JsonGetInt(loot));
    int changed = 0; int index;
    for (index=0;index<JsonGetLength(items);index++) changed += MgLiveNoDrop(StringToObject(JsonGetString(JsonArrayGet(items,index))),JsonGetInt(drop));
    json value = JsonObjectSet(JsonObject(), "loot", JsonBool(GetLootable(target)));
    return JsonObjectSet(MgLiveResult(id), "value", JsonObjectSet(value, "item_updates", JsonInt(changed)));
}
