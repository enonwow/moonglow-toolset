#include "mglive_nui"

void main()
{
    object pc = NuiGetEventPlayer();
    int token = NuiGetEventWindow();
    if (NuiGetWindowId(pc, token) == MGLIVE_SKILL_WINDOW)
    {
        if(token!=GetLocalInt(pc,"MGLIVE_SKILL_TOKEN")) return;
        if(NuiGetEventType()=="click" && NuiGetEventElement()=="roll") MgLiveSkillRoll(pc);
        if(NuiGetEventType()=="click" && NuiGetEventElement()=="ok") NuiDestroy(pc,token);
        return;
    }
    if (NuiGetWindowId(pc, token) == MGLIVE_MESSAGE_WINDOW)
    {
        if (NuiGetEventType() == "click" && NuiGetEventElement() == "ok") NuiDestroy(pc, token);
        return;
    }
    // Preserve the module's existing NUI handler, including non-click events.
    string previous = GetLocalString(GetModule(), "MGLIVE_NUI_PREVIOUS");
    if (previous != "" && previous != "mglive_nuievt") ExecuteScript(previous, GetModule());
}
