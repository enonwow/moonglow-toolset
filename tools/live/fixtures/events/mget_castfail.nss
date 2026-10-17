#include "nwnx_events"
void main()
{
    string reason = NWNX_Events_GetEventData("REASON");
    WriteTimestampedLogEntry("MGET_CAST_FAILED reason=" + reason + " spell=" + NWNX_Events_GetEventData("SPELL_ID"));
    SetName(OBJECT_SELF, "cast-failed-" + reason);
}
