// Compiled VM context: NWNX calls must not run through ExecuteScriptChunk.
#include "mglive_xl"
#include "nwnx_object"
void main() {
    object m = GetModule();
    object speaker = GetLocalObject(m, "MGET_XL_SPEAKER");
    int channel = GetLocalInt(m, "MGET_XL_CHANNEL");
    string label = GetLocalString(m, "MGET_XL_LABEL");
    object target = GetObjectByTag("XL_NEAR");
    // CreateObject/collision placement must not redefine the range case under
    // load. Pin the translated listener in this same VM, before timing capture.
    float before = GetDistanceBetween(speaker,target);
    NWNX_Object_SetPosition(target,GetPosition(speaker)+Vector(0.5,0.0,0.0),FALSE);
    json expected=JsonArray();json clients=MgXlClients();int i;
    for(i=0;i<JsonGetLength(clients);i++)
    {
        object pc=StringToObject(JsonGetString(JsonArrayGet(clients,i)));
        if(MgXlHears(channel,speaker,target,pc))
            JsonArrayInsertInplace(expected,JsonString(ObjectToString(pc)));
    }
    object npc=GetObjectByTag("XL_NPC");
    if((channel==1 || channel==2 || channel==3) && GetIsListening(npc) && MgXlHears(channel,speaker,target,npc))
        JsonArrayInsertInplace(expected,JsonString(ObjectToString(npc)));
    DeleteLocalJson(m,"MGET_XL_SENT_"+label);
    string text = label + " hello";
    int limit = NWNX_Util_GetInstructionLimit();
    int start = NWNX_Util_GetInstructionsExecuted();
    int captured = MgXlCapture(channel, text, speaker, target);
    int used = NWNX_Util_GetInstructionsExecuted() - start;
    json row = JsonObjectSet(JsonObject(), "instructions", JsonInt(used));
    JsonObjectSetInplace(row, "captured", JsonInt(captured));
    JsonObjectSetInplace(row, "limit", JsonInt(limit));
    JsonObjectSetInplace(row, "channel", JsonInt(channel));
    JsonObjectSetInplace(row, "label", JsonString(label));
    JsonObjectSetInplace(row,"target_distance_before",JsonFloat(before));
    JsonObjectSetInplace(row,"target_distance",JsonFloat(GetDistanceBetween(speaker,target)));
    JsonObjectSetInplace(row,"target_range",JsonFloat(NWNX_Chat_GetChatHearingDistance(target,channel)));
    JsonObjectSetInplace(row,"target_hears",JsonBool(MgXlHears(channel,speaker,target,target)));
    JsonObjectSetInplace(row,"expected",expected);
    SetLocalJson(m, "MGET_XL_BUDGET", row);
}
