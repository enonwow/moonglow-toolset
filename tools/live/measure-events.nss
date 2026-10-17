// Private fixture contract; never installed by the production launcher.
#include "mglive_evt"

void main()
{
    object m = GetModule();
    MgLiveClearEvents(GetLocalJson(m, "MGLIVE_EVENTS"));
    int dropped = GetLocalInt(m, "MGLIVE_DROPPED");
    int first = GetLocalInt(m, "MGLIVE_EVENT_SEQ");
    int ok = JsonGetLength(GetLocalJson(m, "MGLIVE_EVENTS")) == 0;
    MgLiveClearEvents(JsonArray());
    int i;
    json e = JsonObjectSet(JsonObject(), "kind", JsonString("chat"));
    e = MgLiveTextField(e, "text", "Zażółć gęślą jaźń / escaped \"quote\" / backslash \\ / newline\n");
    for (i = 0; i < 10; i++) MgLiveQueueEvent(e);
    json all = GetLocalJson(m, "MGLIVE_EVENTS");
    json sent = JsonArray();
    for (i = 0; i < 5; i++) sent = JsonArrayInsert(sent, JsonArrayGet(all, i));
    // Simulate publication callbacks: later events must survive clearing.
    for (i = 0; i < 3; i++) MgLiveQueueEvent(e);
    MgLiveClearEvents(sent);
    json tail = GetLocalJson(m, "MGLIVE_EVENTS");
    ok = ok && JsonGetLength(tail) == 8;
    for (i = 0; i < JsonGetLength(tail); i++)
        ok = ok && JsonGetInt(JsonObjectGet(JsonArrayGet(tail, i), "seq")) == first + 6 + i;
    MgLiveClearEvents(sent); // retry must not clear later events
    ok = ok && JsonDump(tail) == JsonDump(GetLocalJson(m, "MGLIVE_EVENTS"));
    MgLiveClearEvents(tail);
    ok = ok && JsonGetLength(GetLocalJson(m, "MGLIVE_EVENTS")) == 0;
    first = GetLocalInt(m, "MGLIVE_EVENT_SEQ");
    for (i = 0; i < 300; i++) MgLiveQueueEvent(e);
    all = GetLocalJson(m, "MGLIVE_EVENTS");
    ok = ok && JsonGetLength(all) == 256 && GetLocalInt(m, "MGLIVE_DROPPED") == dropped + 44;
    for (i = 0; i < JsonGetLength(all); i++)
    {
        json row = JsonArrayGet(all, i);
        ok = ok && JsonGetInt(JsonObjectGet(row, "seq")) == first + 45 + i;
        ok = ok && JsonGetString(JsonObjectGet(row, "text")) == JsonGetString(JsonObjectGet(e, "text"));
    }
    // A publication callback can overflow the complete sent prefix. Clearing
    // that old snapshot must retain every newer event, including after wrap.
    for (i = 0; i < 300; i++) MgLiveQueueEvent(e);
    tail = GetLocalJson(m, "MGLIVE_EVENTS");
    MgLiveClearEvents(all);
    ok = ok && JsonDump(tail) == JsonDump(GetLocalJson(m, "MGLIVE_EVENTS"));
    ok = ok && GetLocalInt(m, "MGLIVE_DROPPED") == dropped + 344;
    MgLiveClearEvents(tail);
    ok = ok && JsonGetLength(GetLocalJson(m, "MGLIVE_EVENTS")) == 0;
    // Ring storage, when used, must release cleared JSON values too.
    for (i = 0; i < 256; i++)
        ok = ok && JsonGetType(GetLocalJson(m, "MGLIVE_EVENT_SLOT_" + IntToString(i))) == JSON_TYPE_NULL;
    WriteTimestampedLogEntry("MP_EVENT_CHECK pass=" + IntToString(ok));
}
