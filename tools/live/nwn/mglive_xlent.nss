#include "mglive_xllang"
void main()
{
    object pc=GetEnteringObject();
    string previous=GetLocalString(GetModule(),"MGLIVE_XL_ENTER_PREVIOUS");
    if(previous!="" && previous!="mglive_xlent") ExecuteScript(previous,GetModule());
    MgXlLanguageRequest(pc);
    MgXlLanguageOpen(pc);
}
