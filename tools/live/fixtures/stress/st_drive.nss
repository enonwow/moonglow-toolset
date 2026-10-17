#include "nwnx_object"
void main()
{
    object m=GetModule();if(!GetLocalInt(m,"ST_RUNNING")) return;
    int n=GetLocalInt(m,"ST_STEP")+1;SetLocalInt(m,"ST_STEP",n);
    object actor=GetObjectByTag("ST_"+IntToString(n%5)+"_"+IntToString(10+n%20));
    ExecuteScript("st_say",actor);
    ExecuteScript("st_say",GetObjectByTag("ST_"+IntToString((n+1)%5)+"_"+IntToString(10+(n+7)%20)));
    if(n%200==0) ExecuteScript("st_wave",GetObjectByTag("ST_0_40"));
    if(n%600==0)
    {
        object talker=GetObjectByTag("ST_1_40");
        NWNX_Object_SetDialogResref(talker,"mget_focus");
        AssignCommand(talker,ActionStartConversation(GetObjectByTag("ST_1_41"),"mget_focus",FALSE,FALSE));
    }
    DelayCommand(0.05,ExecuteScript("st_drive",m));
}
