#include "mglive_dialog"
int StartingConditional()
{
    // This must never enqueue an offered node, even when its condition is true.
    SetLocalInt(GetModule(),"MGET_FOCUS_COND_CAPTURED",MgLiveDialogLine("mget_focus"));
    SetLocalInt(GetModule(),"MGET_FOCUS_CONDITIONS",GetLocalInt(GetModule(),"MGET_FOCUS_CONDITIONS")+1);
    return TRUE;
}
