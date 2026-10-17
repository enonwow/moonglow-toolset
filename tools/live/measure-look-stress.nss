// Private native serialization/VM regression. No fake PC flag or connected client.
#include "mglive_inc"
void main()
{
    json raw = ObjectToJson(OBJECT_SELF);
    json result = JsonObject();
    JsonObjectSetInplace(result, "raw_bytes", JsonInt(GetStringLength(JsonDump(raw))));
    JsonObjectSetInplace(result, "inventory", JsonInt(JsonGetLength(JsonObjectGet(JsonObjectGet(raw, "ItemList"), "value"))));
    JsonObjectSetInplace(result, "equipment", JsonInt(JsonGetLength(JsonObjectGet(JsonObjectGet(raw, "Equip_ItemList"), "value"))));
    JsonObjectSetInplace(result, "locals", JsonInt(JsonGetLength(JsonObjectGet(JsonObjectGet(raw, "VarTable"), "value"))));
    int start = NWNX_Util_GetInstructionsExecuted();
    json look = MgLiveLook(OBJECT_SELF);
    int used = NWNX_Util_GetInstructionsExecuted() - start;
    JsonObjectSetInplace(result, "look_instructions", JsonInt(used));
    JsonObjectSetInplace(result, "look_bytes", JsonInt(GetStringLength(JsonDump(look))));
    JsonObjectSetInplace(result, "look_equipment", JsonInt(JsonGetLength(JsonObjectGet(JsonObjectGet(look, "Equip_ItemList"), "value"))));
    // A future unknown text field must not trigger a traversal of numeric lists.
    json row = JsonObject(); int i;
    for(i=0;i<20;i++) JsonObjectSetInplace(row, "n"+IntToString(i), JsonObjectSet(JsonObjectSet(JsonObject(),"type",JsonString("float")),"value",JsonFloat(1.0)));
    json rows = JsonArray();
    for(i=0;i<1000;i++) JsonArrayInsertInplace(rows,row);
    json future = JsonObjectSet(JsonObject(),"UnknownText",JsonObjectSet(JsonObjectSet(JsonObject(),"type",JsonString("cexostring")),"value",JsonString("future prose")));
    JsonObjectSetInplace(future,"UnknownList",JsonObjectSet(JsonObjectSet(JsonObject(),"type",JsonString("list")),"value",rows));
    start = NWNX_Util_GetInstructionsExecuted();
    json cleaned = MgLiveLookNoText(future);
    JsonObjectSetInplace(result,"fixed_pass_instructions",JsonInt(NWNX_Util_GetInstructionsExecuted()-start));
    JsonObjectSetInplace(result,"unknown_list_rows",JsonInt(JsonGetLength(JsonObjectGet(JsonObjectGet(cleaned,"UnknownList"),"value"))));
    JsonObjectSetInplace(result,"limit",JsonInt(NWNX_Util_GetInstructionLimit()));
    SetLocalJson(GetModule(),"MP_LOOK_STRESS",result);
}
