// Module-wide observation control. Validate the whole batch before changing flags.
json MgLiveWatchAreas(json j)
{
    string id=JsonGetString(JsonObjectGet(j,"id"));
    json areas=JsonObjectGet(j,"areas"); json on=JsonObjectGet(j,"on");
    int count=JsonGetLength(areas); int i;
    json resolved=JsonObject();
    if(JsonGetType(areas)!=JSON_TYPE_ARRAY || count<1 || count>64 || JsonGetType(on)!=JSON_TYPE_BOOL)
        return MgLiveResult(id,"areas must contain 1-64 resrefs and on must be boolean");
    for(i=0;i<count;i++)
    {
        json item=JsonArrayGet(areas,i);string name=GetStringLowerCase(JsonGetString(item));
        if(JsonGetType(item)!=JSON_TYPE_STRING || !MgLiveResRef(name)) return MgLiveResult(id,"invalid area resref");
        if(JsonGetType(JsonObjectGet(resolved,name))!=JSON_TYPE_NULL) return MgLiveResult(id,"duplicate watched area");
        JsonObjectSetInplace(resolved,name,JsonBool(FALSE));
    }
    // Scan a large module once rather than searching it twice per requested area.
    object found=GetFirstArea();
    while(GetIsObjectValid(found))
    {
        string key=GetStringLowerCase(GetResRef(found));
        if(JsonGetType(JsonObjectGet(resolved,key))==JSON_TYPE_BOOL) JsonObjectSetInplace(resolved,key,JsonString(ObjectToString(found)));
        found=GetNextArea();
    }
    for(i=0;i<count;i++)
    {
        string name=GetStringLowerCase(JsonGetString(JsonArrayGet(areas,i)));
        if(JsonGetType(JsonObjectGet(resolved,name))!=JSON_TYPE_STRING) return MgLiveResult(id,"unknown area: "+name,OBJECT_INVALID,"not_found");
    }
    for(i=0;i<count;i++)
    {
        object area=StringToObject(JsonGetString(JsonObjectGet(resolved,GetStringLowerCase(JsonGetString(JsonArrayGet(areas,i))))));
        if(JsonGetInt(on)) SetLocalInt(area,"MGLIVE_WATCHED",TRUE);
        else DeleteLocalInt(area,"MGLIVE_WATCHED");
    }
    // A newly watched area needs all object kinds and a valid delta baseline.
    SetLocalInt(GetModule(),"MGLIVE_RESYNC",TRUE);
    return MgLiveResult(id);
}
