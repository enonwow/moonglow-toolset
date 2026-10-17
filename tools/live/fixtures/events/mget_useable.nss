void main()
{
    json saved = ObjectToJson(OBJECT_SELF);
    json keys = JsonObjectKeys(saved);
    string probe = "useable:";
    int i;
    for (i = 0; i < JsonGetLength(keys); i++)
    {
        string key = JsonGetString(JsonArrayGet(keys, i));
        if (FindSubString(GetStringLowerCase(key), "us") >= 0)
            probe += key + "=" + JsonDump(JsonObjectGet(saved, key)) + ";";
    }
    SetName(OBJECT_SELF, probe);
}
