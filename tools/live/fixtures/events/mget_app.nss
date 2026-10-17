#include "nwnx_object"
void main()
{
    NWNX_Object_SetAppearance(OBJECT_SELF, 5);
    SetLocalInt(OBJECT_SELF,"MGET_APP_NATIVE",NWNX_Object_GetAppearance(OBJECT_SELF));
}
