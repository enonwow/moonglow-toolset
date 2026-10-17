void main()
{
    json snapshot = ObjectToJson(OBJECT_SELF);
    json field = JsonObjectGet(snapshot, "FactionID");
    SetName(OBJECT_SELF, "native-faction=" + IntToString(JsonGetInt(JsonObjectGet(field, "value"))));
}
