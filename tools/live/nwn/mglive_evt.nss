// Shared game-event buffer. Sequence and overflow counts are scoped to MGLIVE_BOOT.
#include "nwnx_util"
#include "mglive_text"
const int MGLIVE_MAX_EVENTS = 256;
// Each flush carries at most 256. Keep a short bounded backlog so a burst or
// a brief Redis outage can drain over subsequent fresh VM entries.
const int MGLIVE_EVENT_CAPACITY = 2048;

// A VM exception cannot leave a permanent attribution context. Cleanup runs
// next frame, independently of the command stack that might overflow.
void MgLiveClearCommandContext(string id)
{
    object m=GetModule();
    if(GetLocalString(m,"MGLIVE_COMMAND_ID")==id)
    { DeleteLocalString(m,"MGLIVE_COMMAND_ID");DeleteLocalString(m,"MGLIVE_COMMAND_BY"); }
}

json MgLiveEvent(string kind, object source = OBJECT_INVALID, object target = OBJECT_INVALID)
{
    struct NWNX_Util_HighResTimestamp now = NWNX_Util_GetHighResTimeStamp();
    string fraction = "000000" + IntToString(now.microseconds);
    json e = JsonObjectSet(JsonObject(), "kind", JsonString(kind));
    e = JsonObjectSet(e, "time", JsonString(IntToString(now.seconds) + "." + GetStringRight(fraction, 6)));
    // Null means unavailable; module IDs are not character IDs.
    int valid = GetIsObjectValid(source) && source != GetModule();
    e = JsonObjectSet(e, "id", JsonNull());
    e = JsonObjectSet(e, "name", JsonString(""));
    e = JsonObjectSet(e, "area", JsonNull());
    if (valid)
    {
        e = JsonObjectSet(e, "id", JsonString(ObjectToString(source)));
        e = MgLiveTextField(e, "name", GetName(source));
        object area = GetArea(source);
        if (GetIsObjectValid(area)) e = JsonObjectSet(e, "area", JsonString(GetResRef(area)));
    }
    e = JsonObjectSet(e, "target", JsonNull());
    if (GetIsObjectValid(target)) e = JsonObjectSet(e, "target", JsonString(ObjectToString(target)));
    string commandId=GetLocalString(GetModule(),"MGLIVE_COMMAND_ID");
    int active=FALSE;
    if(commandId!="")
    {
        // Native stack inspection also works inside ExecuteScriptChunk, where
        // NWNX Util callbacks can return empty values. Unnamed frames are valid.
        int depth=GetScriptRecursionLevel();int checked=0;
        while(depth>=0 && checked<32)
        {
            if(GetScriptName(depth)=="mglive_ps") { active=TRUE;break; }
            depth--;checked++;
        }
    }
    if(active)
    {
        JsonObjectSetInplace(e,"command_id",JsonString(commandId));
        string by=GetLocalString(GetModule(),"MGLIVE_COMMAND_BY");
        if(by!="") e=MgLiveTextField(e,"by",by);
    }
    return e;
}

// Bounded per-slot storage keeps enqueue independent of the buffered JSON size.
// Each flush snapshot is assembled in sequence order.
json MgLivePendingEvents()
{
    object m = GetModule();
    int count = GetLocalInt(m, "MGLIVE_EVENT_COUNT");
    if (!count) return JsonArray();
    if (count > MGLIVE_MAX_EVENTS) count = MGLIVE_MAX_EVENTS;
    int head = GetLocalInt(m, "MGLIVE_EVENT_HEAD");
    string rows = "";
    int i;
    for (i = 0; i < count; i++)
    {
        if (i) rows += ",";
        rows += JsonDump(GetLocalJson(m, "MGLIVE_EVENT_SLOT_" + IntToString((head + i) % MGLIVE_EVENT_CAPACITY)));
    }
    return JsonParse("[" + rows + "]");
}

json MgLiveQueueEvent(json e)
{
    object m = GetModule();
    if (!GetLocalInt(m, "MGLIVE_STARTED")) return JsonNull();
    int seq = GetLocalInt(m, "MGLIVE_EVENT_SEQ") + 1;
    SetLocalInt(m, "MGLIVE_EVENT_SEQ", seq);
    e = JsonObjectSet(e, "seq", JsonInt(seq));
    int count = GetLocalInt(m, "MGLIVE_EVENT_COUNT");
    int head = GetLocalInt(m, "MGLIVE_EVENT_HEAD");
    int slot = (head + count) % MGLIVE_EVENT_CAPACITY;
    if (count == MGLIVE_EVENT_CAPACITY)
    {
        SetLocalInt(m, "MGLIVE_EVENT_HEAD", (head + 1) % MGLIVE_EVENT_CAPACITY);
        SetLocalInt(m, "MGLIVE_DROPPED", GetLocalInt(m, "MGLIVE_DROPPED") + 1);
    }
    else SetLocalInt(m, "MGLIVE_EVENT_COUNT", count + 1);
    SetLocalJson(m, "MGLIVE_EVENT_SLOT_" + IntToString(slot), e);
    // A fresh VM entry bounds flush cost independently of the event callback.
    if (!GetLocalInt(m, "MGLIVE_EVENT_FLUSH"))
    {
        SetLocalInt(m, "MGLIVE_EVENT_FLUSH", TRUE);
        DelayCommand(0.0, ExecuteScript("mglive_evflush", m));
    }
    return e;
}

void MgLiveClearEvents(json sent)
{
    int n = JsonGetLength(sent);
    if (!n) return;
    int last = JsonGetInt(JsonObjectGet(JsonArrayGet(sent, n - 1), "seq"));
    object m = GetModule();
    int count = GetLocalInt(m, "MGLIVE_EVENT_COUNT");
    int head = GetLocalInt(m, "MGLIVE_EVENT_HEAD");
    // Preserve anything enqueued during publication/reentrant callbacks.
    while (count > 0)
    {
        string key = "MGLIVE_EVENT_SLOT_" + IntToString(head);
        if (JsonGetInt(JsonObjectGet(GetLocalJson(m, key), "seq")) > last) break;
        DeleteLocalJson(m, key);
        head = (head + 1) % MGLIVE_EVENT_CAPACITY;
        count--;
    }
    SetLocalInt(m, "MGLIVE_EVENT_HEAD", head);
    SetLocalInt(m, "MGLIVE_EVENT_COUNT", count);
}

void MgLiveObserveDeath(object o, string commandId="", string by="")
{
    if (!GetIsObjectValid(o) || GetObjectType(o) != OBJECT_TYPE_CREATURE) return;
    if (!GetIsDead(o))
    {
        if (GetLocalInt(o, "MGLIVE_DEATH_REPORTED")) DeleteLocalObject(o, "MGLIVE_DAMAGER");
        DeleteLocalInt(o, "MGLIVE_DEATH_REPORTED");
        return;
    }
    if (GetLocalInt(o, "MGLIVE_DEATH_REPORTED")) return;
    SetLocalInt(o, "MGLIVE_DEATH_REPORTED", TRUE);
    json e = MgLiveEvent("death", o);
    if(commandId!="") e=JsonObjectSet(e,"command_id",JsonString(commandId));
    if(by!="") e=MgLiveTextField(e,"by",by);
    e = JsonObjectSet(e, "hp", JsonInt(GetCurrentHitPoints(o)));
    object killer = GetLocalObject(o, "MGLIVE_DAMAGER");
    e = JsonObjectSet(e, "killer", JsonNull());
    if (GetIsObjectValid(killer)) e = JsonObjectSet(e, "killer", JsonString(ObjectToString(killer)));
    MgLiveQueueEvent(e);
}
