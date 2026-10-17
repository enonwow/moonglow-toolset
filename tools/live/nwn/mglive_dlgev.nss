// For a previously empty Actions Taken slot: set the mglive_dialog script
// parameter to the actual DLG resref. Existing scripts should call the include
// helper directly, preserving their own behavior and parameters.
#include "mglive_dialog"
void main()
{
    MgLiveDialogLine(GetScriptParam("mglive_dialog"));
}
