// Area builders frozen from b249a4c; delta oracle from93f90bc. Private tests only.
json MpReferenceArea(object area, int bFull)
{
    string cre = "";
    string plc = "";
    string doors = "";
    string items = "";
    object o = GetFirstObjectInArea(area);
    while (GetIsObjectValid(o))
    {
        int t = GetObjectType(o);
        if (t == OBJECT_TYPE_CREATURE)
            cre += JsonDump(MgLiveObject(o, bFull)) + ",";
        else if (bFull && t == OBJECT_TYPE_PLACEABLE)
            plc += JsonDump(MgLiveObject(o, TRUE)) + ",";
        else if (bFull && t == OBJECT_TYPE_DOOR)
            doors += JsonDump(MgLiveObject(o, TRUE)) + ",";
        else if (bFull && t == OBJECT_TYPE_ITEM)
            items += JsonDump(MgLiveObject(o, TRUE)) + ",";
        o = GetNextObjectInArea(area);
    }
    string data = "{\"area\":" + JsonDump(JsonString(GetResRef(area))) +
        ",\"size\":[" + IntToString(GetAreaSize(AREA_WIDTH, area)*10) + "," +
        IntToString(GetAreaSize(AREA_HEIGHT, area)*10) + "],\"full\":";
    if (bFull) data += "true";
    else data += "false";
    data += ",\"cre\":" + MgLiveArrayText(cre);
    if (bFull)
    {
        data += ",\"plc\":" + MgLiveArrayText(plc) +
            ",\"doors\":" + MgLiveArrayText(doors) + ",\"items\":" + MgLiveArrayText(items);
    }
    return JsonParse(data + "}");
}

// Selection by resref, never by tag; visit each area only once.
json MpReferenceAreas(int bFull)
{
    object module = GetModule();
    string forced = "," + GetStringLowerCase(GetLocalString(module, "MGLIVE_AREAS")) + ",";
    string areas = "";
    object a = GetFirstArea();
    while (GetIsObjectValid(a))
    {
        int wanted = GetLocalInt(a, "MGLIVE_WATCHED") || FindSubString(forced, "," + GetStringLowerCase(GetResRef(a)) + ",") >= 0;
        object pc = GetFirstPC();
        while (!wanted && GetIsObjectValid(pc))
        {
            wanted = GetArea(pc) == a;
            pc = GetNextPC();
        }
        if (wanted) areas += JsonDump(MpReferenceArea(a, bFull)) + ",";
        a = GetNextArea();
    }
    return JsonParse(MgLiveArrayText(areas));
}

json MpReferenceDeltaAreas(json areas, int full, int seq)
{
    object m = GetModule();
    int ai;
    for (ai = 0; ai < JsonGetLength(areas); ai++)
    {
        json area = JsonArrayGet(areas, ai);
        string name = JsonGetString(JsonObjectGet(area, "area"));
        string key = "MP_REFERENCE_HASH_" + name;
        json before = GetLocalJson(m, key);
        if (JsonGetType(before) != JSON_TYPE_OBJECT) before = JsonObject();
        json after = JsonObject();
        int areaFull = full || GetLocalInt(m, "MP_REFERENCE_SEEN_" + name) != seq - 1;
        int k;
        for (k = 0; k < 4; k++)
        {
            string kind = "cre";
            if (k == 1) kind = "plc";
            else if (k == 2) kind = "doors";
            else if (k == 3) kind = "items";
            json rows = JsonObjectGet(area, kind);
            json changed = JsonArray();
            int i;
            for (i = 0; i < JsonGetLength(rows); i++)
            {
                json row = JsonArrayGet(rows, i);
                string id = JsonGetString(JsonObjectGet(row, "id"));
                int hash = HashString(JsonDump(row));
                json previous = JsonObjectGet(before, id);
                if (areaFull || JsonGetType(previous) != JSON_TYPE_INTEGER || JsonGetInt(previous) != hash)
                    changed = JsonArrayInsert(changed, row);
                after = JsonObjectSet(after, id, JsonInt(hash));
            }
            area = JsonObjectSet(area, kind, changed);
        }
        json removed = JsonArray();
        if (!areaFull)
        {
            json ids = JsonObjectKeys(before);
            int i;
            for (i = 0; i < JsonGetLength(ids); i++)
            {
                string id = JsonGetString(JsonArrayGet(ids, i));
                if (JsonGetType(JsonObjectGet(after, id)) == JSON_TYPE_NULL)
                    removed = JsonArrayInsert(removed, JsonString(id));
            }
        }
        area = JsonObjectSet(area, "full", JsonBool(areaFull));
        area = JsonObjectSet(area, "removed", removed);
        areas = JsonArraySet(areas, ai, area);
        SetLocalJson(m, key, after);
        SetLocalInt(m, "MP_REFERENCE_SEEN_" + name, seq);
    }
    return areas;
}
