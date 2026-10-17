#include "mglive_exec"
void main()
{
    object m=GetModule();int n=GetLocalInt(m,"ST_SETUP");
    if(n<6)
    {
        object actor=GetObjectByTag("ST_"+IntToString(n%5)+"_"+IntToString(n/5));
        SetLocalInt(m,"MP_SCALE_PHASE",0);ExecuteScript("mp_scale",actor);
        int i;for(i=0;i<200;i++) CreateItemOnObject("nw_it_mring001",actor);
        SetLocalInt(actor,"ST_PC_LOOK",TRUE);ExecuteScript("mp_stress",actor);
        json looks=GetLocalJson(m,"ST_LOOKS");if(n==0) looks=JsonArray();
        JsonArrayInsertInplace(looks,GetLocalJson(m,"MP_LOOK_STRESS"));SetLocalJson(m,"ST_LOOKS",looks);
        SetLocalInt(m,"ST_SETUP",n+1);DelayCommand(0.1,ExecuteScript("st_setup",m));return;
    }
    int a;int i;
    for(a=0;a<5;a++) for(i=10;i<15;i++)
    {
        object actor=GetObjectByTag("ST_"+IntToString(a)+"_"+IntToString(i));vector p=GetPosition(actor);
        string points="[{\"x\":"+FloatToString(p.x+2.0)+",\"y\":"+FloatToString(p.y+2.0)+",\"z\":0},{\"x\":"+FloatToString(p.x-2.0)+",\"y\":"+FloatToString(p.y+2.0)+",\"z\":0}]";
        json command=JsonParse("{\"id\":\"setup-"+IntToString(a)+"-"+IntToString(i)+"\",\"area\":\""+GetResRef(GetArea(actor))+"\",\"target\":\""+ObjectToString(actor)+"\",\"op\":\"patrol\",\"points\":"+points+",\"loop\":true,\"run\":false}");
        json result=MgLiveRun(command);if(!JsonGetInt(JsonObjectGet(result,"ok"))) WriteTimestampedLogEntry("ST_SETUP_FAILED "+JsonDump(result));
    }
    SetLocalInt(m,"ST_READY",TRUE);
}
