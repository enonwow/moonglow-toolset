#include "nwnx_util"
void main()
{
    int start=NWNX_Util_GetInstructionsExecuted();
    WriteTimestampedLogEntry("MP_VM "+GetLocalString(GetModule(),"MP_VM_PHASE")+"="+IntToString(start));
}
