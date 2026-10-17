// Private native comparison with the frozen pre-optimization implementation.
#include "mglive_inc"
#include "mp_reference"

// Sparse looks are a wire omission only. Rehydrate the unchanged hash from
// the baseline before comparing with the frozen full-row delta oracle.
json MpRestoreLooks(json delta, json baseline)
{
    int a; int i; int b;
    for(a=0;a<JsonGetLength(delta);a++)
    {
        json area=JsonArrayGet(delta,a);
        json rows=JsonObjectGet(area,"cre");
        json old=JsonObjectGet(JsonArrayGet(baseline,a),"cre");
        for(i=0;i<JsonGetLength(rows);i++)
        {
            json row=JsonArrayGet(rows,i);
            if(JsonGetType(JsonObjectGet(row,"look"))!=JSON_TYPE_NULL) continue;
            for(b=0;b<JsonGetLength(old);b++)
            {
                json previous=JsonArrayGet(old,b);
                if(JsonDump(JsonObjectGet(row,"id"))==JsonDump(JsonObjectGet(previous,"id")) &&
                   JsonDump(JsonObjectGet(row,"lookHash"))==JsonDump(JsonObjectGet(previous,"lookHash")))
                {
                    row=JsonObjectSet(row,"look",JsonObjectGet(previous,"look"));
                    rows=JsonArraySet(rows,i,row);
                    break;
                }
            }
        }
        area=JsonObjectSet(area,"cre",rows);
        delta=JsonArraySet(delta,a,area);
    }
    return delta;
}

void main()
{
    json rows = MgLiveAreas(TRUE);
    int ok = JsonDump(rows) == JsonDump(MpReferenceAreas(TRUE));
    ok = ok && JsonDump(MgLiveAreas(FALSE)) == JsonDump(MpReferenceAreas(FALSE));
    json actual = MgLiveDeltaAreas(rows, TRUE, 1000);
    json expected = MpReferenceDeltaAreas(rows, TRUE, 1000);
    ok = ok && JsonDump(actual) == JsonDump(expected);
    actual = MgLiveDeltaAreas(rows, FALSE, 1001);
    expected = MpReferenceDeltaAreas(rows, FALSE, 1001);
    ok = ok && JsonDump(actual) == JsonDump(expected);
    json baseline = rows;
    // A changed row, disappearing ID and an object changing collections.
    json area = JsonArrayGet(rows, 0);
    json cre = JsonObjectGet(area, "cre");
    json row = JsonArrayGet(cre, 0);
    row = MgLiveTextField(row, "name", "Zażółć gęślą jaźń / quote \" / slash \\ / newline\n");
    cre = JsonArraySet(cre, 0, row);
    json moved = JsonArrayGet(cre, 1);
    cre = JsonArrayDel(cre, 1);
    cre = JsonArrayDel(cre, 1);
    area = JsonObjectSet(area, "cre", cre);
    area = JsonObjectSet(area, "items", JsonArrayInsert(JsonObjectGet(area, "items"), moved));
    rows = JsonArraySet(rows, 0, area);
    actual = MgLiveDeltaAreas(rows, FALSE, 1002);
    expected = MpReferenceDeltaAreas(rows, FALSE, 1002);
    ok = ok && JsonDump(MpRestoreLooks(actual, baseline)) == JsonDump(expected);
    // A skipped area must receive a full per-area baseline again.
    actual = MgLiveDeltaAreas(rows, FALSE, 1004);
    expected = MpReferenceDeltaAreas(rows, FALSE, 1004);
    ok = ok && JsonDump(actual) == JsonDump(expected);
    WriteTimestampedLogEntry("MP_REPORT_CHECK pass=" + IntToString(ok));
}
