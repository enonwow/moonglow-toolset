// Bounded, versioned results. PUBLISH subscriber counts are not receipts.
const int MGLIVE_RESULT_LIMIT = 256;
const int MGLIVE_RESULT_TICKS = 300;
// Leave space for the state baseline inside the receiver's report limit.
const int MGLIVE_REPORT_RESULT_BYTES = 524288;

json MgLiveReportResults()
{
    json retained=GetLocalJson(GetModule(),"MGLIVE_RESULTS");
    json batch=JsonArray();int bytes=2;int i;
    for(i=0;i<JsonGetLength(retained);i++)
    {
        json result=JsonArrayGet(retained,i);
        int size=GetStringLength(JsonDump(result))+1;
        if(bytes+size>MGLIVE_REPORT_RESULT_BYTES) break;
        batch=JsonArrayInsert(batch,result);bytes+=size;
    }
    return batch;
}

void MgLiveRetainResult(json answer)
{
    object m=GetModule();
    int serial=GetLocalInt(m,"MGLIVE_RESULT_SERIAL")+1;
    SetLocalInt(m,"MGLIVE_RESULT_SERIAL",serial);
    answer=JsonObjectSet(answer,"receipt",JsonString(GetLocalString(m,"MGLIVE_BOOT")+":"+IntToString(serial)));
    answer=JsonObjectSet(answer,"expires",JsonInt(GetLocalInt(m,"MGLIVE_SEQ")+MGLIVE_RESULT_TICKS));
    json results=GetLocalJson(m,"MGLIVE_RESULTS");
    string id=JsonGetString(JsonObjectGet(answer,"id"));int i;
    for(i=JsonGetLength(results)-1;i>=0;i--)
        if(JsonGetString(JsonObjectGet(JsonArrayGet(results,i),"id"))==id) results=JsonArrayDel(results,i);
    results=JsonArrayInsert(results,answer);
    while(JsonGetLength(results)>MGLIVE_RESULT_LIMIT || GetStringLength(JsonDump(results))>2097152)
        results=JsonArrayDel(results,0);
    SetLocalJson(m,"MGLIVE_RESULTS",results);
    json ack=GetLocalJson(m,"MGLIVE_ACK");
    if(JsonGetType(JsonFind(ack,JsonString(id)))==JSON_TYPE_NULL) ack=JsonArrayInsert(ack,JsonString(id));
    while(JsonGetLength(ack)>MGLIVE_RESULT_LIMIT) ack=JsonArrayDel(ack,0);
    SetLocalJson(m,"MGLIVE_ACK",ack);
}

void MgLiveReceiveResults(json receipts)
{
    object m=GetModule();json results=GetLocalJson(m,"MGLIVE_RESULTS");
    json ack=GetLocalJson(m,"MGLIVE_ACK");int i;
    for(i=JsonGetLength(results)-1;i>=0;i--)
    {
        json result=JsonArrayGet(results,i);
        json receipt=JsonObjectGet(result,"receipt");
        int received=JsonGetType(receipt)==JSON_TYPE_STRING &&
            JsonGetType(receipts)==JSON_TYPE_ARRAY &&
            JsonGetType(JsonFind(receipts,receipt))!=JSON_TYPE_NULL;
        if(received || JsonGetInt(JsonObjectGet(result,"expires"))<=GetLocalInt(m,"MGLIVE_SEQ"))
        {
            // Expiry preserves the bounded ACK: a missing outcome stays unknown.
            if(received)
            {
                json at=JsonFind(ack,JsonObjectGet(result,"id"));
                if(JsonGetType(at)!=JSON_TYPE_NULL) ack=JsonArrayDel(ack,JsonGetInt(at));
            }
            results=JsonArrayDel(results,i);
        }
    }
    SetLocalJson(m,"MGLIVE_RESULTS",results);SetLocalJson(m,"MGLIVE_ACK",ack);
}
