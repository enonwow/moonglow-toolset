// Trust-boundary shape checks before the native GFF loader. The Rust publisher
// additionally runs the checked mg-gff decoder (ranges, labels, base64, strings).
int MgLiveGffStruct(json value, int depth = 0)
{
    if (depth > 15 || JsonGetType(value) != JSON_TYPE_OBJECT) return FALSE;
    json keys = JsonObjectKeys(value);
    if (JsonGetLength(keys) > 1024) return FALSE;
    int i;
    for (i = 0; i < JsonGetLength(keys); i++)
    {
        string key = JsonGetString(JsonArrayGet(keys, i));
        if (key == "__data_type" || key == "__struct_id") continue;
        if (GetStringLength(key) > 16 || key == "") return FALSE;
        json field = JsonObjectGet(value, key);
        if (JsonGetType(field) != JSON_TYPE_OBJECT) return FALSE;
        string type = JsonGetString(JsonObjectGet(field, "type"));
        SetLocalString(GetModule(), "MGLIVE_JSON_FIELD", key + ":" + type);
        json data = JsonObjectGet(field, "value");
        int dt = JsonGetType(data);
        if (type == "struct")
        {
            if (!MgLiveGffStruct(data, depth+1)) return FALSE;
        }
        else if (type == "list")
        {
            if (dt != JSON_TYPE_ARRAY || JsonGetLength(data) > 1024) return FALSE;
            int n;
            for (n = 0; n < JsonGetLength(data); n++)
                if (!MgLiveGffStruct(JsonArrayGet(data, n), depth+1)) return FALSE;
        }
        else if (type == "cexostring" || type == "resref")
        {
            if (dt != JSON_TYPE_STRING) return FALSE;
            if (type == "resref" && GetStringLength(JsonGetString(data)) > 16) return FALSE;
        }
        else if (type == "cexolocstring")
        {
            if (dt != JSON_TYPE_OBJECT) return FALSE;
            json strings = JsonObjectKeys(data);
            int n;
            for (n = 0; n < JsonGetLength(strings); n++)
            {
                string lang = JsonGetString(JsonArrayGet(strings, n));
                int kind = JsonGetType(JsonObjectGet(data, lang));
                if (lang == "id") { if (kind != JSON_TYPE_INTEGER) return FALSE; }
                else if (kind != JSON_TYPE_STRING) return FALSE;
            }
        }
        else if (type == "void")
        {
            if (dt != JSON_TYPE_STRING && JsonGetType(JsonObjectGet(field, "value64")) != JSON_TYPE_STRING) return FALSE;
        }
        else if (type == "float" || type == "double")
        {
            if (!MgLiveNumber(data)) return FALSE;
        }
        else if (type == "byte") { if (!MgLiveInt(data, 0, 255)) return FALSE; }
        else if (type == "char") { if (!MgLiveInt(data, -128, 127)) return FALSE; }
        else if (type == "word") { if (!MgLiveInt(data, 0, 65535)) return FALSE; }
        else if (type == "short") { if (!MgLiveInt(data, -32768, 32767)) return FALSE; }
        else if (type == "int") { if (!MgLiveInt(data, -2147483647 - 1, 2147483647)) return FALSE; }
        else if (type == "dword" || type == "dword64" || type == "int64")
        {
            if (dt != JSON_TYPE_INTEGER) return FALSE;
            if (type != "int64" && GetStringLeft(JsonDump(data), 1) == "-") return FALSE;
            if (type == "dword" && JsonGetFloat(data) > 4294967295.0) return FALSE;
        }
        else return FALSE;
    }
    return TRUE;
}

int MgLiveObjectJson(json value)
{
    SetLocalString(GetModule(), "MGLIVE_JSON_FIELD", "size/type");
    if (GetStringLength(JsonDump(value)) > 262144) return FALSE;
    string type = JsonGetString(JsonObjectGet(value, "__data_type"));
    if (type != "UTC " && type != "UTP " && type != "UTD " && type != "UTI ") return FALSE;
    if (!MgLiveGffStruct(value)) return FALSE;
    if (JsonGetInt(JsonObjectGet(JsonObjectGet(value, "IsPC"), "value")) ||
        JsonGetInt(JsonObjectGet(JsonObjectGet(value, "IsDM"), "value"))) return FALSE;
    return TRUE;
}
