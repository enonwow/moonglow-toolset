// Automatically included in run_chunk when return:true is requested.
// s is JSON text, e.g. MgReturn(JsonDump(JsonInt(42))). Last synchronous call wins.
void MgReturn(string s)
{
    object m = GetModule();
    if (!GetLocalInt(m, "MGLIVE_RETURN_ACTIVE")) return;
    DeleteLocalString(m, "MGLIVE_RETURN_ERROR");
    if (GetStringLength(s) > 16384)
    {
        SetLocalString(m, "MGLIVE_RETURN_ERROR", "return value exceeds 16384 JSON bytes");
        return;
    }
    json value = JsonParse(s);
    if (JsonGetError(value) != "")
    {
        SetLocalString(m, "MGLIVE_RETURN_ERROR", "return value is not valid JSON");
        return;
    }
    s = JsonDump(value);
    if (GetStringLength(s) > 16384)
        SetLocalString(m, "MGLIVE_RETURN_ERROR", "return value exceeds 16384 JSON bytes");
    else SetLocalString(m, "MGLIVE_RETURN", s);
}
