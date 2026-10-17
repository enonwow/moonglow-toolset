#include "nwnx_util"
void main()
{
    object m=GetModule(); int i=0;
    while(i<10000) i++;
    WriteTimestampedLogEntry("MP_VM before="+IntToString(NWNX_Util_GetInstructionsExecuted()));
    SetLocalString(m,"MP_VM_PHASE","nested");
    ExecuteScript("mp_vmchild",m);
    WriteTimestampedLogEntry("MP_VM after="+IntToString(NWNX_Util_GetInstructionsExecuted()));
    SetLocalString(m,"MP_VM_PHASE","delayed");
    DelayCommand(0.1,ExecuteScript("mp_vmchild",m));
}
