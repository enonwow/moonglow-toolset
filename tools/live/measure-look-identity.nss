// Fixed-size native look regression: no client or arbitrary tree traversal.
#include "mglive_inc"
void main()
{
    object actor = GetObjectByTag("C15_ACTOR");
    int appearance = GetAppearanceType(actor);
    float facing = GetFacing(actor);
    json look = MgLiveLook(actor);
    int first = HashString(JsonDump(look));
    SetFacing(facing + 35.0);
    int turned = HashString(JsonDump(MgLiveLook(actor)));
    SetCreatureAppearanceType(actor, appearance == 6 ? 7 : 6);
    int changed = HashString(JsonDump(MgLiveLook(actor)));
    SetCreatureAppearanceType(actor, appearance);
    SetFacing(facing);
    json result = JsonObject();
    JsonObjectSetInplace(result, "first", JsonInt(first));
    JsonObjectSetInplace(result, "turned", JsonInt(turned));
    JsonObjectSetInplace(result, "changed", JsonInt(changed));
    JsonObjectSetInplace(result, "restored", JsonInt(HashString(JsonDump(MgLiveLook(actor)))));
    JsonObjectSetInplace(result, "metadata", JsonObjectGet(look, "__data_type"));
    JsonObjectSetInplace(result, "equipment", JsonInt(JsonGetLength(JsonObjectGet(JsonObjectGet(look,"Equip_ItemList"),"value"))));
    SetLocalJson(GetModule(), "MP_LOOK_IDENTITY", result);
}
