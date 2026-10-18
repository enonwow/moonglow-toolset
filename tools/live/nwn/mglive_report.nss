// One report, built in bounded per-area VM entries. The wire envelope is unchanged.
#include "mglive_inc"
#include "nwnx_redis"
#include "mglive_evt"

void MgLiveReportPublish()
{
    object m=GetModule();int seq=GetLocalInt(m,"MGLIVE_SEQ");
    int full=GetLocalInt(m,"MGLIVE_REPORT_FULL");int delta=GetLocalInt(m,"MGLIVE_DELTA");
    int start=GetMicrosecondCounter();string rows="";int i;int count=GetLocalInt(m,"MGLIVE_REPORT_COUNT");
    for(i=0;i<count;i++)
    {
        string key="MGLIVE_REPORT_"+IntToString(i);
        if(i) rows+=",";rows+=JsonDump(GetLocalJson(m,key));DeleteLocalJson(m,key);DeleteLocalObject(m,key);
    }
    json areas=JsonParse("["+rows+"]");
    MgLiveReceiveResults(JsonArray());
    json ack = GetLocalJson(m, "MGLIVE_ACK");
    json results = GetLocalJson(m, "MGLIVE_RESULTS");
    json body = JsonObjectSet(JsonObject(), "v", JsonInt(1));
    body = JsonObjectSet(body, "text_codepage", JsonString(MgLiveTextCodepage()));
    body = JsonObjectSet(body, "boot", JsonString(GetLocalString(m, "MGLIVE_BOOT")));
    body = JsonObjectSet(body, "seq", JsonInt(seq));
    body = JsonObjectSet(body, "watched_areas", GetLocalJson(m,"MGLIVE_WATCHED_AREAS"));
    body = JsonObjectSet(body, "factions", GetLocalJson(m, "MGLIVE_FACTIONS"));
    body = JsonObjectSet(body, "ack", ack);
    body = JsonObjectSet(body, "results", results);
    if (delta && !full) body = JsonObjectSet(body, "base_seq", JsonInt(seq - 1));
    body = JsonObjectSet(body, "event_stream", JsonBool(TRUE));
    body = JsonObjectSet(body, "dropped", JsonInt(GetLocalInt(m, "MGLIVE_DROPPED")));
    // Attach the large collection last, so small envelope fields never copy it.
    body = JsonObjectSet(body, "areas", areas);
    string data = JsonDump(body);
    int r = NWNX_Redis_PUBLISH(GetLocalString(m, "MGLIVE_PREFIX") + ".state", data);
    int heard = NWNX_Redis_GetResultAsInt(r);
    // Results remain until explicit versioned receipts or bounded expiry.
    int total=GetLocalInt(m,"MGLIVE_REPORT_US")+GetMicrosecondCounter()-start;
    if(seq==1 || seq%60==0)
        WriteTimestampedLogEntry("MGLIVE: seq="+IntToString(seq)+" full="+IntToString(full)+" tick_us="+IntToString(total)+" bytes="+IntToString(GetStringLength(data))+" heard="+IntToString(heard));
    DeleteLocalInt(m,"MGLIVE_REPORT_BUSY");
}

void MgLiveReportStep()
{
    object m=GetModule();if(!GetLocalInt(m,"MGLIVE_REPORT_BUSY")) return;
    int start=GetMicrosecondCounter();int i=GetLocalInt(m,"MGLIVE_REPORT_AT");
    int phase=GetLocalInt(m,"MGLIVE_REPORT_PHASE");int count=GetLocalInt(m,"MGLIVE_REPORT_COUNT");
    int seq=GetLocalInt(m,"MGLIVE_SEQ");int full=GetLocalInt(m,"MGLIVE_REPORT_FULL");
    int delta=GetLocalInt(m,"MGLIVE_DELTA");
    if(i<count)
    {
        string key="MGLIVE_REPORT_"+IntToString(i);json area;
        if(phase==0)
        {
            area=MgLiveArea(GetLocalObject(m,key),full || delta);
            if(full && !delta) SetLocalInt(m,"MGLIVE_REPORT_BYTES",GetLocalInt(m,"MGLIVE_REPORT_BYTES")+GetStringLength(JsonDump(area)));
        }
        else area=GetLocalJson(m,key);
        if(delta) area=JsonArrayGet(MgLiveDeltaAreas(JsonArrayInsert(JsonArray(),area),full,seq),0);
        SetLocalJson(m,key,area);SetLocalInt(m,"MGLIVE_REPORT_AT",i+1);
        SetLocalInt(m,"MGLIVE_REPORT_US",GetLocalInt(m,"MGLIVE_REPORT_US")+GetMicrosecondCounter()-start);
        DelayCommand(0.0,ExecuteScript("mglive_rpt",m));return;
    }
    if(phase==0 && !delta && full && GetLocalInt(m,"MGLIVE_REPORT_BYTES")>=MGLIVE_DELTA_THRESHOLD)
    {
        SetLocalInt(m,"MGLIVE_DELTA",TRUE);SetLocalInt(m,"MGLIVE_REPORT_PHASE",1);SetLocalInt(m,"MGLIVE_REPORT_AT",0);
        DelayCommand(0.0,ExecuteScript("mglive_rpt",m));return;
    }
    MgLiveReportPublish();
}

void MgLiveReportBegin()
{
    object m=GetModule();if(!GetLocalInt(m,"MGLIVE_STARTED")) return;
    if(GetLocalInt(m,"MGLIVE_REPORT_BUSY")) { DelayCommand(0.1,ExecuteScript("mglive_tick",m));return; }
    int start=GetMicrosecondCounter();
    if(!MgLiveLooksReady()) { DelayCommand(0.1,ExecuteScript("mglive_tick",m));return; }
    DelayCommand(MGLIVE_PERIOD,ExecuteScript("mglive_tick",m));
    int seq=GetLocalInt(m,"MGLIVE_SEQ")+1;SetLocalInt(m,"MGLIVE_SEQ",seq);
    int delta=GetLocalInt(m,"MGLIVE_DELTA");int full=seq==1 || GetLocalInt(m,"MGLIVE_RESYNC");
    if(delta) full=full || seq-GetLocalInt(m,"MGLIVE_LAST_FULL")>=MGLIVE_DELTA_FULL_EVERY;
    else full=full || seq%MGLIVE_FULL_EVERY==0;
    DeleteLocalInt(m,"MGLIVE_RESYNC");if(full) SetLocalInt(m,"MGLIVE_LAST_FULL",seq);
    SetLocalInt(m,"MGLIVE_REPORT_FULL",full);SetLocalInt(m,"MGLIVE_REPORT_BUSY",TRUE);
    SetLocalInt(m,"MGLIVE_REPORT_AT",0);SetLocalInt(m,"MGLIVE_REPORT_PHASE",0);
    SetLocalInt(m,"MGLIVE_REPORT_US",GetMicrosecondCounter()-start);SetLocalInt(m,"MGLIVE_REPORT_BYTES",0);
    json watched=JsonArray();
    int count=0;object area=GetFirstArea();
    while(GetIsObjectValid(area))
    {
        if(GetLocalInt(area,"MGLIVE_WATCHED")) watched=JsonArrayInsert(watched,JsonString(GetResRef(area)));
        if(MgLiveAreaWanted(area)) { SetLocalObject(m,"MGLIVE_REPORT_"+IntToString(count),area);count++; }
        area=GetNextArea();
    }
    SetLocalJson(m,"MGLIVE_WATCHED_AREAS",watched);
    SetLocalInt(m,"MGLIVE_REPORT_COUNT",count);DelayCommand(0.0,ExecuteScript("mglive_rpt",m));
}
