// Included after mglive_exec's result helper. Reserved locals are transport
// state, permissions or NWNX internals, never agent-editable application data.
int MgVarReserved(string name)
{
    name = GetStringUpperCase(name);
    return GetStringLeft(name, 7) == "MGLIVE_" || GetStringLeft(name, 5) == "NWNX_";
}

string MgVarType(int type)
{
    if (type == 1) return "int";
    if (type == 2) return "float";
    if (type == 3) return "string";
    if (type == 4) return "object";
    if (type == 5) return "location";
    if (type == 6) return "json";
    return "unknown";
}

json MgVarValue(object target, string name, string type)
{
    if (type == "int") return JsonInt(GetLocalInt(target, name));
    if (type == "float") return JsonFloat(GetLocalFloat(target, name));
    if (type == "string") return JsonString(GetLocalString(target, name));
    if (type == "json") return GetLocalJson(target, name);
    if (type == "object")
    {
        object value = GetLocalObject(target, name);
        if (!GetIsObjectValid(value)) return JsonNull();
        return JsonString(ObjectToString(value));
    }
    if (type == "location")
    {
        location at = GetLocalLocation(target, name);
        object area = GetAreaFromLocation(at);
        if (!GetIsObjectValid(area)) return JsonNull();
        vector p = GetPositionFromLocation(at);
        json value = JsonObjectSet(JsonObject(), "area", JsonString(GetResRef(area)));
        value = JsonObjectSet(value, "x", JsonFloat(p.x));
        value = JsonObjectSet(value, "y", JsonFloat(p.y));
        value = JsonObjectSet(value, "z", JsonFloat(p.z));
        return JsonObjectSet(value, "f", JsonFloat(GetFacingFromLocation(at)));
    }
    return JsonNull();
}

json MgLiveVariables(json j, object area, int allowPlayers)
{
    string id = JsonGetString(JsonObjectGet(j, "id"));
    string op = JsonGetString(JsonObjectGet(j, "op"));
    string requested = JsonGetString(JsonObjectGet(j, "target"));
    object target;
    if (requested == "module") target = GetModule();
    else if (requested == "area") target = area;
    else
    {
        if (!MgLiveObjectId(requested)) return MgLiveResult(id, "invalid target");
        target = MgLiveFindObject(requested);
        if (!GetIsObjectValid(target)) return MgLiveResult(id, "unknown target", OBJECT_INVALID, "not_found");
        if (GetArea(target) != area) return MgLiveResult(id, "target in another area", OBJECT_INVALID, "other_area");
    }
    if (op == "set_variable" && MgLiveProtected(target) && !allowPlayers)
        return MgLiveResult(id, "protected target", OBJECT_INVALID, "protected");
    if (op == "dump_locals")
    {
        int count = NWNX_Object_GetLocalVariableCount(target);
        int i; int omitted; int visited;
        json rows = JsonArray();
        for (i = 0; i < count && i < 256; i++)
        {
            struct NWNX_Object_LocalVariable local = NWNX_Object_GetLocalVariable(target, i);
            visited++;
            if (MgVarReserved(local.key)) { omitted++; continue; }
            json row = JsonObjectSet(JsonObject(), "name", JsonString(local.key));
            row = JsonObjectSet(row, "type", JsonString(MgVarType(local.type)));
            row = JsonObjectSet(row, "value", MgVarValue(target, local.key, MgVarType(local.type)));
            json candidate = JsonArrayInsert(rows, row);
            if (GetStringLength(JsonDump(candidate)) > 15000 || JsonGetLength(candidate) > 128)
            { visited--; break; }
            rows = candidate;
        }
        json result = JsonObjectSet(JsonObject(), "locals", rows);
        result = JsonObjectSet(result, "total", JsonInt(count));
        result = JsonObjectSet(result, "omitted", JsonInt(omitted));
        result = JsonObjectSet(result, "truncated", JsonBool(visited < count));
        return JsonObjectSet(MgLiveResult(id), "value", result);
    }
    string name = JsonGetString(JsonObjectGet(j, "name"));
    if (GetStringLength(name) < 1 || GetStringLength(name) > 128) return MgLiveResult(id, "name must be 1-128 bytes");
    int c;
    for (c = 0; c < GetStringLength(name); c++)
        if (FindSubString("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_. -", GetSubString(name, c, 1)) < 0) return MgLiveResult(id, "invalid local name");
    if (MgVarReserved(name)) return MgLiveResult(id, "reserved local");
    string type = JsonGetString(JsonObjectGet(j, "type"));
    if (type != "int" && type != "float" && type != "string" && type != "object" && type != "location")
        return MgLiveResult(id, "unsupported local type");
    if (op == "set_variable")
    {
        json value = JsonObjectGet(j, "value");
        int kind = JsonGetType(value);
        if (type == "int")
        {
            if (kind != JSON_TYPE_INTEGER || JsonDump(value) != IntToString(JsonGetInt(value))) return MgLiveResult(id, "integer required");
            SetLocalInt(target, name, JsonGetInt(value));
        }
        else if (type == "float")
        {
            if (!MgLiveNumber(value)) return MgLiveResult(id, "finite float required");
            SetLocalFloat(target, name, JsonGetFloat(value));
        }
        else if (type == "string")
        {
            if (kind != JSON_TYPE_STRING || GetStringLength(JsonGetString(value)) > 4096) return MgLiveResult(id, "string must be <=4096 bytes");
            SetLocalString(target, name, JsonGetString(value));
        }
        else if (type == "object")
        {
            object objectValue = OBJECT_INVALID;
            if (kind != JSON_TYPE_NULL)
            {
                if (kind != JSON_TYPE_STRING || !MgLiveObjectId(JsonGetString(value))) return MgLiveResult(id, "object id or null required");
                objectValue = MgLiveFindObject(JsonGetString(value));
                if (!GetIsObjectValid(objectValue)) return MgLiveResult(id, "value object not found", OBJECT_INVALID, "not_found");
            }
            SetLocalObject(target, name, objectValue);
        }
        else
        {
            if (kind == JSON_TYPE_NULL) DeleteLocalLocation(target, name);
            else
            {
                if (kind != JSON_TYPE_OBJECT || !MgLivePosition(value) || !MgLiveNumber(JsonObjectGet(value, "f"))) return MgLiveResult(id, "location requires area,x,y,z,f");
                object dest = MgLiveFindArea(JsonGetString(JsonObjectGet(value, "area")));
                if (!GetIsObjectValid(dest)) return MgLiveResult(id, "location area not found", OBJECT_INVALID, "not_found");
                SetLocalLocation(target, name, Location(dest, Vector(JsonGetFloat(JsonObjectGet(value, "x")), JsonGetFloat(JsonObjectGet(value, "y")), JsonGetFloat(JsonObjectGet(value, "z"))), JsonGetFloat(JsonObjectGet(value, "f"))));
            }
        }
    }
    json returned = MgVarValue(target, name, type);
    if (GetStringLength(JsonDump(returned)) > 16384) return MgLiveResult(id, "local exceeds 16KB result limit");
    return JsonObjectSet(MgLiveResult(id), "value", returned);
}
