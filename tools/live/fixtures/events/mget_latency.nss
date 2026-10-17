// Private native timing source. Twenty messages span different tick phases.
#include "nwnx_chat"
#include "nwnx_redis"
void Send(int i)
{
    string text="latency-"+IntToString(i);
    // Fixture-only origin marker: host measures marker and event on one clock.
    NWNX_Redis_PUBLISH("mglive-events.latency-marker",text);
    NWNX_Chat_SendMessage(1,text,OBJECT_SELF);
}
void main()
{
    int i;
    for (i=0; i<20; i++)
        DelayCommand(2.0+IntToFloat(i)*0.413, Send(i));
}
