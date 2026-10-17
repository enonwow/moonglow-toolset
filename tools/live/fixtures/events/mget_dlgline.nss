#include "mglive_dialog"
void main()
{
    object m=GetModule();
    int captured;
    if(GetLocalInt(m,"MGET_FOCUS_EXPLICIT"))
    {
        object other=GetLocalObject(m,"MGET_FOCUS_LISTENER");
        if(NWNX_Dialog_GetCurrentNodeType()==NWNX_DIALOG_NODE_TYPE_REPLY_NODE)
            captured=MgLiveDialogLine("mget_focus",other,OBJECT_SELF);
        else captured=MgLiveDialogLine("mget_focus",OBJECT_SELF,other);
    }
    else captured=MgLiveDialogLine("mget_focus");
    SetLocalInt(m,"MGET_FOCUS_ACTIONS",GetLocalInt(m,"MGET_FOCUS_ACTIONS")+1);
    SetLocalInt(m,"MGET_FOCUS_CAPTURED",GetLocalInt(m,"MGET_FOCUS_CAPTURED")+captured);
    SetLocalInt(m,"MGET_FOCUS_TYPE",NWNX_Dialog_GetCurrentNodeType());
    SetLocalInt(m,"MGET_FOCUS_SCRIPT",NWNX_Dialog_GetCurrentScriptType());
    SetLocalString(m,"MGET_FOCUS_PARAM",GetScriptParam("preserved"));
    SetLocalObject(m,"MGET_FOCUS_OWNER",OBJECT_SELF);
    SetLocalObject(m,"MGET_FOCUS_LAST",GetLastSpeaker());
    SetLocalObject(m,"MGET_FOCUS_PC",GetPCSpeaker());
}
