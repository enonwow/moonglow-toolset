// Bounded per-area snapshots; partial coverage is explicit in every area row.
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
    MgLiveRecoverCommand();
    MgLiveReceiveResults(JsonArray());
    json results = MgLiveReportResults();
    // Deferred results must not look like lost outcomes via an early ACK.
    json ack = JsonArray();
    for(i=0;i<JsonGetLength(results);i++)
        ack=JsonArrayInsert(ack,JsonObjectGet(JsonArrayGet(results,i),"id"));
    json body = JsonObjectSet(JsonObject(), "v", JsonInt(1));
    body = JsonObjectSet(body, "script_version", JsonInt(2));
    body = JsonObjectSet(body, "text_codepage", JsonString(MgLiveTextCodepage()));
    body = JsonObjectSet(body, "prefix", JsonString(GetLocalString(m,"MGLIVE_PREFIX")));
    body = JsonObjectSet(body, "boot", JsonString(GetLocalString(m, "MGLIVE_BOOT")));
    body = JsonObjectSet(body, "seq", JsonInt(seq));
    body = JsonObjectSet(body, "translation_config", JsonString(GetLocalString(m,"MGLIVE_XL_CONFIG")));
    body = JsonObjectSet(body, "watched_areas", GetLocalJson(m,"MGLIVE_WATCHED_AREAS"));
    body = JsonObjectSet(body, "factions", GetLocalJson(m, "MGLIVE_FACTIONS"));
    body = JsonObjectSet(body, "ack", ack);
    body = JsonObjectSet(body, "results", results);
    if (delta && !full) body = JsonObjectSet(body, "base_seq", JsonInt(seq - 1));
    body = JsonObjectSet(body, "event_stream", JsonBool(TRUE));
    body = JsonObjectSet(body, "dropped", JsonInt(GetLocalInt(m, "MGLIVE_DROPPED")));
    body = JsonObjectSet(body,"report_recovered",JsonInt(GetLocalInt(m,"MGLIVE_REPORT_RECOVERED")));
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

// A VM abort cannot run cleanup. The tick watchdog advances past the failed
// area using an explicitly incomplete snapshot, so command receipts still flow.
void MgLiveReportSkip()
{
    object m=GetModule();int i=GetLocalInt(m,"MGLIVE_REPORT_AT");
    string key="MGLIVE_REPORT_"+IntToString(i);
    json area=JsonObjectSet(JsonObject(),"area",JsonString(GetResRef(GetLocalObject(m,key))));
    JsonObjectSetInplace(area,"full",JsonBool(TRUE));
    JsonObjectSetInplace(area,"paged",JsonBool(TRUE));
    JsonObjectSetInplace(area,"cre_partial",JsonBool(TRUE));
    // No page means preserve previously received statics and restart this scan.
    SetLocalInt(GetLocalObject(m,key),"MGLIVE_PAGE_INDEX",0);
    SetLocalInt(m,"MGLIVE_PAGE_PENDING",TRUE);
    SetLocalJson(m,key,area);SetLocalInt(m,"MGLIVE_REPORT_AT",i+1);
    DeleteLocalInt(m,"MGLIVE_REPORT_WARM");
    SetLocalInt(m,"MGLIVE_REPORT_RECOVERED",GetLocalInt(m,"MGLIVE_REPORT_RECOVERED")+1);
    // Reset the hash baseline, including when the failure was in delta encoding.
    DeleteLocalInt(m,"MGLIVE_SEEN_"+GetResRef(GetLocalObject(m,key)));
    DelayCommand(0.0,ExecuteScript("mglive_rpt",m));
}

void MgLiveReportStep()
{
    object m=GetModule();if(!GetLocalInt(m,"MGLIVE_REPORT_BUSY")) return;
    SetLocalInt(m,"MGLIVE_REPORT_PROGRESS",GetLocalInt(m,"MGLIVE_REPORT_PROGRESS")+1);
    int start=GetMicrosecondCounter();int i=GetLocalInt(m,"MGLIVE_REPORT_AT");
    int phase=GetLocalInt(m,"MGLIVE_REPORT_PHASE");int count=GetLocalInt(m,"MGLIVE_REPORT_COUNT");
    int seq=GetLocalInt(m,"MGLIVE_SEQ");int full=GetLocalInt(m,"MGLIVE_REPORT_FULL");
    int delta=GetLocalInt(m,"MGLIVE_DELTA");
    if(i<count)
    {
        string key="MGLIVE_REPORT_"+IntToString(i);json area;
        if(phase==0)
        {
            area=MgLiveArea(GetLocalObject(m,key),full);
            if(full && !delta) SetLocalInt(m,"MGLIVE_REPORT_BYTES",GetLocalInt(m,"MGLIVE_REPORT_BYTES")+GetStringLength(JsonDump(area)));
        }
        else area=GetLocalJson(m,key);
        if(phase==1) area=JsonArrayGet(MgLiveDeltaAreas(JsonArrayInsert(JsonArray(),area),full,seq),0);
        SetLocalJson(m,key,area);SetLocalInt(m,"MGLIVE_REPORT_AT",i+1);
        SetLocalInt(m,"MGLIVE_REPORT_US",GetLocalInt(m,"MGLIVE_REPORT_US")+GetMicrosecondCounter()-start);
        DelayCommand(0.0,ExecuteScript("mglive_rpt",m));return;
    }
    if(phase==0 && (delta || (full && GetLocalInt(m,"MGLIVE_REPORT_BYTES")>=MGLIVE_DELTA_THRESHOLD)))
    {
        SetLocalInt(m,"MGLIVE_DELTA",TRUE);SetLocalInt(m,"MGLIVE_REPORT_PHASE",1);SetLocalInt(m,"MGLIVE_REPORT_AT",0);
        DelayCommand(0.0,ExecuteScript("mglive_rpt",m));return;
    }
    MgLiveReportPublish();
}

void MgLiveReportBegin()
{
    object m=GetModule();if(!GetLocalInt(m,"MGLIVE_STARTED")) return;
    if(GetLocalInt(m,"MGLIVE_REPORT_BUSY"))
    {
        int progress=GetLocalInt(m,"MGLIVE_REPORT_PROGRESS");
        int stalled=0;
        if(progress==GetLocalInt(m,"MGLIVE_REPORT_LAST_PROGRESS"))
            stalled=GetLocalInt(m,"MGLIVE_REPORT_STALLED")+1;
        SetLocalInt(m,"MGLIVE_REPORT_LAST_PROGRESS",progress);
        SetLocalInt(m,"MGLIVE_REPORT_STALLED",stalled);
        if(stalled>=3)
        {
            DeleteLocalInt(m,"MGLIVE_REPORT_STALLED");
            WriteTimestampedLogEntry("MGLIVE: recovering an interrupted area report");
            if(GetLocalInt(m,"MGLIVE_REPORT_AT")<GetLocalInt(m,"MGLIVE_REPORT_COUNT")) MgLiveReportSkip();
            else
            {
                // If envelope publication itself was interrupted, send only
                // receipts now and rebuild every area on the next full report.
                SetLocalInt(m,"MGLIVE_REPORT_COUNT",0);
                SetLocalInt(m,"MGLIVE_RESYNC",TRUE);
                SetLocalInt(m,"MGLIVE_REPORT_RECOVERED",GetLocalInt(m,"MGLIVE_REPORT_RECOVERED")+1);
                MgLiveReportPublish();
            }
        }
        DelayCommand(MGLIVE_PERIOD,ExecuteScript("mglive_tick",m));return;
    }
    int start=GetMicrosecondCounter();
    DelayCommand(MGLIVE_PERIOD,ExecuteScript("mglive_tick",m));
    int seq=GetLocalInt(m,"MGLIVE_SEQ")+1;SetLocalInt(m,"MGLIVE_SEQ",seq);
    int delta=GetLocalInt(m,"MGLIVE_DELTA");int full=seq==1 || GetLocalInt(m,"MGLIVE_RESYNC") || GetLocalInt(m,"MGLIVE_PAGE_PENDING");
    if(delta) full=full || seq-GetLocalInt(m,"MGLIVE_LAST_FULL")>=MGLIVE_DELTA_FULL_EVERY;
    else full=full || seq%MGLIVE_FULL_EVERY==0;
    // A resync starts new scan generations; a normal continuation retains cursors.
    if(GetLocalInt(m,"MGLIVE_RESYNC"))
    {
        object reset=GetFirstArea();
        while(GetIsObjectValid(reset)) { SetLocalInt(reset,"MGLIVE_PAGE_INDEX",0);reset=GetNextArea(); }
    }
    DeleteLocalInt(m,"MGLIVE_PAGE_PENDING");DeleteLocalInt(m,"MGLIVE_LOOK_JOBS");
    DeleteLocalInt(m,"MGLIVE_RESYNC");if(full) SetLocalInt(m,"MGLIVE_LAST_FULL",seq);
    SetLocalInt(m,"MGLIVE_REPORT_FULL",full);SetLocalInt(m,"MGLIVE_REPORT_BUSY",TRUE);
    SetLocalInt(m,"MGLIVE_REPORT_AT",0);SetLocalInt(m,"MGLIVE_REPORT_PHASE",0);
    DeleteLocalInt(m,"MGLIVE_REPORT_STALLED");DeleteLocalInt(m,"MGLIVE_REPORT_WARM");
    SetLocalInt(m,"MGLIVE_REPORT_US",GetMicrosecondCounter()-start);SetLocalInt(m,"MGLIVE_REPORT_BYTES",0);
    json watched=JsonArray();
    // Players take priority if they enter new areas after watch admission.
    // Mark each area once so multiple PCs do not consume multiple slots.
    int count=0;int deferred=0;object pc=GetFirstPC();object area;
    while(GetIsObjectValid(pc))
    {
        area=GetArea(pc);
        if(GetIsObjectValid(area) && GetLocalInt(area,"MGLIVE_REPORT_SELECTED")!=seq)
        {
            if(count<64) { SetLocalObject(m,"MGLIVE_REPORT_"+IntToString(count),area);count++; }
            else deferred++;
            SetLocalInt(area,"MGLIVE_REPORT_SELECTED",seq);
        }
        pc=GetNextPC();
    }
    area=GetFirstArea();
    while(GetIsObjectValid(area))
    {
        if(GetLocalInt(area,"MGLIVE_WATCHED")) watched=JsonArrayInsert(watched,JsonString(GetResRef(area)));
        if(MgLiveAreaWanted(area) && GetLocalInt(area,"MGLIVE_REPORT_SELECTED")!=seq)
        {
            if(count<64) { SetLocalObject(m,"MGLIVE_REPORT_"+IntToString(count),area);count++; }
            else deferred++;
        }
        area=GetNextArea();
    }
    if(deferred && (seq==1 || seq%60==0))
        WriteTimestampedLogEntry("MGLIVE: watch limit exceeded; deferred areas="+IntToString(deferred));
    SetLocalJson(m,"MGLIVE_WATCHED_AREAS",watched);
    SetLocalInt(m,"MGLIVE_REPORT_COUNT",count);DelayCommand(0.0,ExecuteScript("mglive_rpt",m));
}
