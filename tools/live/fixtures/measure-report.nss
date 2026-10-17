// Detailed component observer only: sequential reference assembly.
// Production scheduling is measured separately by mglive_report/mglive_rpt.
#include "mglive_inc"
#include "nwnx_redis"
#include "mglive_evt"

void main()
{
    object m = GetModule();
    if (!GetLocalInt(m, "MGLIVE_STARTED")) return;
    if (!MgLiveLooksReady())
    {
        DelayCommand(0.1, ExecuteScript("mglive_tick", m));
        return;
    }
    DelayCommand(MGLIVE_PERIOD, ExecuteScript("mglive_tick", m));
    int start = GetMicrosecondCounter();
    int seq = GetLocalInt(m, "MGLIVE_SEQ") + 1;
    SetLocalInt(m, "MGLIVE_SEQ", seq);
    int delta = GetLocalInt(m, "MGLIVE_DELTA");
    int full = seq == 1 || GetLocalInt(m, "MGLIVE_RESYNC");
    if (delta) full = full || seq - GetLocalInt(m, "MGLIVE_LAST_FULL") >= MGLIVE_DELTA_FULL_EVERY;
    else full = full || seq % MGLIVE_FULL_EVERY == 0;
    DeleteLocalInt(m, "MGLIVE_RESYNC");
    json areas = MgLiveAreas(full || delta);
    if (!delta && full && GetStringLength(JsonDump(areas)) >= MGLIVE_DELTA_THRESHOLD)
    {
        delta = TRUE;
        SetLocalInt(m, "MGLIVE_DELTA", TRUE);
    }
    if (full) SetLocalInt(m, "MGLIVE_LAST_FULL", seq);
    if (delta) areas = MgLiveDeltaAreas(areas, full, seq);
    json ack = GetLocalJson(m, "MGLIVE_ACK");
    json results = GetLocalJson(m, "MGLIVE_RESULTS");
    json body = JsonObjectSet(JsonObject(), "v", JsonInt(1));
    body = JsonObjectSet(body, "text_codepage", JsonString(MgLiveTextCodepage()));
    body = JsonObjectSet(body, "boot", JsonString(GetLocalString(m, "MGLIVE_BOOT")));
    body = JsonObjectSet(body, "seq", JsonInt(seq));
    body = JsonObjectSet(body, "factions", GetLocalJson(m, "MGLIVE_FACTIONS"));
    body = JsonObjectSet(body, "ack", ack);
    body = JsonObjectSet(body, "results", results);
    if (delta && !full) body = JsonObjectSet(body, "base_seq", JsonInt(seq - 1));
    json events = MgLivePendingEvents();
    body = JsonObjectSet(body, "events", events);
    body = JsonObjectSet(body, "dropped", JsonInt(GetLocalInt(m, "MGLIVE_DROPPED")));
    // Attach the large collection last, so small envelope fields never copy it.
    body = JsonObjectSet(body, "areas", areas);
    string data = JsonDump(body);
    int r = NWNX_Redis_PUBLISH(GetLocalString(m, "MGLIVE_PREFIX") + ".state", data);
    int heard = NWNX_Redis_GetResultAsInt(r);
    // Pub/sub has no durable delivery: retain these until a subscriber heard them.
    if (heard > 0)
    {
        MgLiveClearSent(ack);
        MgLiveClearEvents(events);
    }
    if (seq == 1 || seq % 60 == 0)
        WriteTimestampedLogEntry("MGLIVE: seq=" + IntToString(seq) + " full=" + IntToString(full) +
            " tick_us=" + IntToString(GetMicrosecondCounter() - start) +
            " bytes=" + IntToString(GetStringLength(data)) + " heard=" + IntToString(heard));
}
