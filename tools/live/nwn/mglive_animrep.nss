// Read native serialized override storage, not client rendering. Keep replies below 16 KB.
json MgLiveVisualState(object target, string id, string field)
{
    json state=JsonObjectGet(ObjectToJson(target),field);
    int readable=JsonGetType(state) != JSON_TYPE_NULL;
    int truncated=GetStringLength(JsonDump(state)) > 15000;
    json value=JsonObjectSet(JsonObject(),"called",JsonBool(TRUE));
    value=JsonObjectSet(value,"readable",JsonBool(readable));
    value=JsonObjectSet(value,"truncated",JsonBool(truncated));
    if (readable && !truncated) value=JsonObjectSet(value,"state",state);
    return JsonObjectSet(MgLiveResult(id),"value",value);
}

json MgLiveReplace(json j, object target, string id)
{
    json old=JsonObjectGet(j,"old"); json next=JsonObjectGet(j,"new"); json reset=JsonObjectGet(j,"reset");
    if (JsonGetType(old) != JSON_TYPE_STRING || !MgLiveResRef(JsonGetString(old))) return MgLiveResult(id,"old must be a 1-16 character resref");
    if (JsonGetType(reset) != JSON_TYPE_NULL && JsonGetType(reset) != JSON_TYPE_BOOL) return MgLiveResult(id,"reset must be boolean");
    int clearing=JsonGetInt(reset);
    if ((clearing && JsonGetType(next) != JSON_TYPE_NULL) || (!clearing && (JsonGetType(next) != JSON_TYPE_STRING || !MgLiveResRef(JsonGetString(next))))) return MgLiveResult(id,"supply new or reset exclusively");
    string replacement=""; if (!clearing) replacement=JsonGetString(next);
    ReplaceObjectAnimation(target,JsonGetString(old),replacement);
    return MgLiveVisualState(target,id,"AnimationReplace");
}
