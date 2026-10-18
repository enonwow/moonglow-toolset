#include "nwnx_redis"
void main()
{
    string text=GetMatchedSubstring(0);
    if(text=="") text=GetMatchedSubstring(1);
    if(GetStringLeft(text,3)!="xl_") return;
    int space=FindSubString(text," ");string id=GetStringLeft(text,space);
    object m=GetModule();int at=GetLocalInt(m,"MGET_XL_AT_"+id);
    json row=JsonObjectSet(JsonObject(),"id",JsonString(id));
    JsonObjectSetInplace(row,"listener",JsonString(GetTag(OBJECT_SELF)));
    JsonObjectSetInplace(row,"text",JsonString(text));
    JsonObjectSetInplace(row,"us",JsonInt(GetMicrosecondCounter()-at));
    JsonObjectSetInplace(row,"translated",JsonBool(FindSubString(text,"translated-")>=0));
    NWNX_Redis_PUBLISH(GetLocalString(m,"MGLIVE_PREFIX")+".xl.proof",JsonDump(row));
}
