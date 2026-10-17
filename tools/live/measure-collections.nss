// Private paired collection timings; one pair per VM invocation.
#include "mglive_inc"
#include "mp_reference"
void main()
{
    MgLiveAreas(TRUE); // warm NPC caches outside the paired boundary
    // Pair both collection builders on the exact same native state and look cache.
    // Alternate order; JSON equality and serialization are outside the timer.
    int pair = GetLocalInt(GetModule(), "MP_PAIR_ORDER");
    SetLocalInt(GetModule(), "MP_PAIR_ORDER", pair + 1);
    {
        json fast;
        json reference;
        int start;
        int beforeUs;
        int afterUs;
        if (pair % 2 == 0)
        {
            start = GetMicrosecondCounter(); reference = MpReferenceAreas(TRUE);
            beforeUs = GetMicrosecondCounter() - start;
            start = GetMicrosecondCounter(); fast = MgLiveAreas(TRUE);
            afterUs = GetMicrosecondCounter() - start;
        }
        else
        {
            start = GetMicrosecondCounter(); fast = MgLiveAreas(TRUE);
            afterUs = GetMicrosecondCounter() - start;
            start = GetMicrosecondCounter(); reference = MpReferenceAreas(TRUE);
            beforeUs = GetMicrosecondCounter() - start;
        }
        int ok = JsonDump(fast) == JsonDump(reference);
        WriteTimestampedLogEntry("MP_COLLECTION pass=" + IntToString(ok) + " before_us=" + IntToString(beforeUs) +
            " after_us=" + IntToString(afterUs) + " bytes=" + IntToString(GetStringLength(JsonDump(fast))));
    }
}
