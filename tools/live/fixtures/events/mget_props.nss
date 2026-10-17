#include "nwnx_object"
void main()
{
    json value = JsonObject();
    if (GetObjectType(OBJECT_SELF) == OBJECT_TYPE_PLACEABLE)
    {
        value=JsonObjectSet(value,"app",JsonInt(NWNX_Object_GetAppearance(OBJECT_SELF)));
        value=JsonObjectSet(value,"static",JsonBool(NWNX_Object_GetPlaceableIsStatic(OBJECT_SELF)));
    }
    SetLocalJson(OBJECT_SELF,"MGET_PROPERTIES",value);
}
