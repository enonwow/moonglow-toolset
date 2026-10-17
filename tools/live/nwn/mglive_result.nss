// JsonGetInt truncates to the VM's 32 bits. Reject wider JSON integers before
// range checks, so values such as 4294967297 cannot masquerade as one.
int MgLiveInt(json value, int lower, int upper)
{
    if (JsonGetType(value) != JSON_TYPE_INTEGER) return FALSE;
    int n = JsonGetInt(value);
    return JsonDump(value) == IntToString(n) && n >= lower && n <= upper;
}

json MgLiveResult(string id, string error = "", object made = OBJECT_INVALID, string code = "invalid")
{
    json j = JsonObjectSet(JsonObject(), "id", JsonString(id));
    j = JsonObjectSet(j, "ok", JsonBool(error == ""));
    if (error != "")
    {
        j = JsonObjectSet(j, "err", JsonString(error));
        j = JsonObjectSet(j, "code", JsonString(code));
    }
    if (GetIsObjectValid(made)) j = JsonObjectSet(j, "obj", JsonString(ObjectToString(made)));
    return j;
}
