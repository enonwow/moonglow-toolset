// The two demo-module lines: inspect bytes before/after the module's
// JsonString -> nested JSON -> JsonGetString route, independently of Redis.
#include "nwnx_util"
void main()
{
    int stage = GetLocalInt(GetModule(), "MGET_MIXED_STAGE");
    string raw = "Naprawdę sama widziałam";
    if (stage % 2) raw = "Teraz już każdy o tym wie";
    if (stage >= 4) raw = GetStringByStrRef(16777217 + stage % 2);
    json sound = JsonObjectSet(JsonObject(), "Text", JsonString(raw));
    json sounds = JsonObjectSet(JsonObject(), "0", sound);
    string roundtrip = JsonGetString(JsonObjectGet(JsonObjectGet(sounds, "0"), "Text"));
    json probe = JsonObjectSet(JsonObject(), "raw", JsonString(NWNX_Util_EncodeStringForURL(raw)));
    probe = JsonObjectSet(probe, "roundtrip", JsonString(NWNX_Util_EncodeStringForURL(roundtrip)));
    probe = JsonObjectSet(probe, "json", JsonString(NWNX_Util_EncodeStringForURL(JsonDump(sound))));
    SetLocalString(GetModule(), "MGET_MIXED_PROBE", JsonDump(probe));
    if (stage >= 2) raw = roundtrip;
    SetName(OBJECT_SELF, raw);
    SpeakString(raw);
    WriteTimestampedLogEntry("MGET_MIXED " + raw);
}
