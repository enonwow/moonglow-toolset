// Private GFF/codepage proof; never installed in a production module.
#include "mglive_inc"
void main()
{
    object o = GetFirstObjectInArea(GetFirstArea());
    while (GetIsObjectValid(o) && GetObjectType(o) != OBJECT_TYPE_CREATURE)
        o = GetNextObjectInArea(GetFirstArea());
    json snapshot = ObjectToJson(o);
    // Keep Unicode in the native JSON value: extracting it into an NWScript
    // string would itself replace unrepresentable characters before this test.
    json description = JsonObjectGet(snapshot, "DescriptionOverr");
    JsonObjectSetInplace(description, "value", JsonParse("\"\\u0105\\u0416\""));
    JsonObjectSetInplace(snapshot, "DescriptionOverr", description);
    json sample = JsonObject();
    JsonObjectSetInplace(sample, "__data_type", JsonObjectGet(snapshot, "__data_type"));
    JsonObjectSetInplace(sample, "__struct_id", JsonInt(0));
    JsonObjectSetInplace(sample, "Appearance_Type", JsonObjectGet(snapshot, "Appearance_Type"));
    JsonObjectSetInplace(sample, "Description", JsonObjectGet(snapshot, "Description"));
    JsonObjectSetInplace(sample, "DescriptionOverr", JsonObjectGet(snapshot, "DescriptionOverr"));
    JsonObjectSetInplace(sample, "ScriptSpawn", JsonObjectGet(snapshot, "ScriptSpawn"));
    // Exercise the same bounded root + visible equipment passes as MgLiveLook.
    json item = JsonObjectSet(JsonObject(), "__struct_id", JsonInt(16));
    JsonObjectSetInplace(item, "DescriptionOverr", JsonObjectGet(snapshot, "DescriptionOverr"));
    JsonObjectSetInplace(item, "ScriptSpawn", JsonObjectGet(snapshot, "ScriptSpawn"));
    json rows = JsonArrayInsert(JsonArray(), item);
    JsonObjectSetInplace(sample, "Equip_ItemList", JsonObjectSet(JsonObjectSet(JsonObject(), "type", JsonString("list")), "value", rows));
    // Inplace native JSON edits can share children: preserve the negative
    // control before cleaning the independent equipment sample.
    json unknown = JsonParse(JsonDump(JsonObjectSet(JsonObject(), "Nested", JsonObjectSet(JsonObjectSet(JsonObject(), "type", JsonString("struct")), "value", item))));
    json result = JsonObjectSet(JsonObject(), "before", JsonParse(JsonDump(sample)));
    json cleanedItem = MgLiveLookNoText(item);
    JsonObjectSetInplace(sample, "Equip_ItemList", JsonObjectSet(JsonObjectSet(JsonObject(), "type", JsonString("list")), "value", JsonArrayInsert(JsonArray(), cleanedItem)));
    JsonObjectSetInplace(result, "after", MgLiveLookNoText(sample));
    // Unknown nested prose belongs to the GUI decoder defence, not a native
    // recursive traversal (which exhausted the VM budget on connected PCs).
    JsonObjectSetInplace(result, "unknown_after", MgLiveLookNoText(unknown));
    SetLocalJson(GetModule(), "MP_LOOK_PROBE", result);
}
