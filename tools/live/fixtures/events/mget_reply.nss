#include "mglive_inc"
#include "mglive_result"
#include "mglive_reply"
void main()
{
    json results=JsonArray();
    JsonArrayInsertInplace(results,MgLiveReplyDeliver("nearest",OBJECT_SELF,JsonNull(),"reply-nearest-proof"));
    object chosen=GetLocalObject(OBJECT_SELF,"MGET_REPLY_AS");
    JsonArrayInsertInplace(results,MgLiveReplyDeliver("explicit",OBJECT_SELF,JsonString(ObjectToString(chosen)),"reply-explicit-proof"));
    SetLocalInt(OBJECT_SELF,"MGLIVE_CHAT_TO_DM",TRUE);
    JsonArrayInsertInplace(results,MgLiveReplyDeliver("dm",OBJECT_SELF,JsonNull(),"reply-dm-proof"));
    DeleteLocalInt(OBJECT_SELF,"MGLIVE_CHAT_TO_DM");
    SetLocalJson(OBJECT_SELF,"MGET_REPLIES",results);
}
