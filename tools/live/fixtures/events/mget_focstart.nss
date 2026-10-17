#include "nwnx_object"
void main()
{
    object listener=GetLocalObject(GetModule(),"MGET_FOCUS_LISTENER");
    if(GetLocalInt(GetModule(),"MGET_FOCUS_ONELINER"))
        SpeakOneLinerConversation("mget_focus",listener);
    else
    {
        NWNX_Object_SetDialogResref(OBJECT_SELF,"mget_focus");
        ActionStartConversation(listener,"mget_focus",FALSE,FALSE);
    }
}
