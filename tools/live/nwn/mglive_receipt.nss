// Bounded, versioned results. PUBLISH subscriber counts are not receipts.
const int MGLIVE_RESULT_LIMIT = 256;
const int MGLIVE_RESULT_TICKS = 300;
// Leave space for the state baseline inside the receiver's report limit.
const int MGLIVE_REPORT_RESULT_BYTES = 524288;

int MgLiveResultBytes(json result)
{
    int cached=JsonGetInt(JsonObjectGet(result,"stored_bytes"));
    if(cached>0) return cached;
    return GetStringLength(JsonDump(result))+1;
}

json MgLiveReportResults()
{
    json retained=GetLocalJson(GetModule(),"MGLIVE_RESULTS");
    json batch=JsonArray();int bytes=2;int i;
    for(i=0;i<JsonGetLength(retained);i++)
    {
        json result=JsonArrayGet(retained,i);
        int size=MgLiveResultBytes(result);
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
    // Cache this row's serialized size (including its comma) once. Fixed-point
    // sizing includes the size field itself, without ever dumping the list.
    answer=JsonObjectSet(answer,"stored_bytes",JsonInt(0));
    for(i=0;i<4;i++)
    {
        int size=GetStringLength(JsonDump(answer))+1;
        if(size==JsonGetInt(JsonObjectGet(answer,"stored_bytes"))) break;
        answer=JsonObjectSet(answer,"stored_bytes",JsonInt(size));
    }
    int bytes=GetLocalInt(m,"MGLIVE_RESULT_BYTES");if(bytes<2) bytes=2;
    for(i=JsonGetLength(results)-1;i>=0;i--)
        if(JsonGetString(JsonObjectGet(JsonArrayGet(results,i),"id"))==id)
        { bytes-=MgLiveResultBytes(JsonArrayGet(results,i));results=JsonArrayDel(results,i); }
    results=JsonArrayInsert(results,answer);
    bytes+=MgLiveResultBytes(answer);
    while(JsonGetLength(results)>MGLIVE_RESULT_LIMIT || bytes>2097152)
    { bytes-=MgLiveResultBytes(JsonArrayGet(results,0));results=JsonArrayDel(results,0); }
    SetLocalInt(m,"MGLIVE_RESULT_BYTES",bytes);
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
    int bytes=GetLocalInt(m,"MGLIVE_RESULT_BYTES");if(bytes<2) bytes=2;
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
            bytes-=MgLiveResultBytes(result);results=JsonArrayDel(results,i);
        }
    }
    if(bytes<2) bytes=2;
    SetLocalInt(m,"MGLIVE_RESULT_BYTES",bytes);
    SetLocalJson(m,"MGLIVE_RESULTS",results);SetLocalJson(m,"MGLIVE_ACK",ack);
}
