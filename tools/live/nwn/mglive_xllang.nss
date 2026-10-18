// Gateway-owned preferences; this script keeps only the current character's copy.
#include "nw_inc_nui"
#include "nwnx_redis"

int MgXlKnownLanguage(string lang)
{
    return lang=="en" || lang=="pl" || lang=="de" || lang=="fr" || lang=="es" || lang=="it";
}

void MgXlLanguageRequest(object pc,string lang="")
{
    if(!GetLocalInt(GetModule(),"MGLIVE_XL") || !GetIsPC(pc)) return;
    string player=GetPCPublicCDKey(pc);
    if(player=="") return;
    int seq=GetLocalInt(GetModule(),"MGLIVE_LANG_SEQ")+1;
    if(seq<=0) return; // Fail closed on wrap; never reuse a request in this boot.
    SetLocalInt(GetModule(),"MGLIVE_LANG_SEQ",seq);
    string req=GetLocalString(GetModule(),"MGLIVE_BOOT")+":"+ObjectToString(pc)+":"+IntToString(seq);
    SetLocalString(pc,"MGLIVE_LANG_REQ",req);
    json row=JsonObjectSet(JsonObject(),"kind",JsonString("language"));
    JsonObjectSetInplace(row,"req",JsonString(req));
    JsonObjectSetInplace(row,"pc",JsonString(ObjectToString(pc)));
    JsonObjectSetInplace(row,"player",JsonString(player));
    if(MgXlKnownLanguage(lang)) JsonObjectSetInplace(row,"lang",JsonString(lang));
    NWNX_Redis_PUBLISH(GetLocalString(GetModule(),"MGLIVE_PREFIX")+".xl.req",JsonDump(row));
}

json MgXlLanguageWindow()
{
    json rows=JsonArray();
    JsonArrayInsertInplace(rows,NuiHeight(NuiId(NuiButton(JsonString("English")),"en"),32.0));
    JsonArrayInsertInplace(rows,NuiHeight(NuiId(NuiButton(JsonString("Polski")),"pl"),32.0));
    JsonArrayInsertInplace(rows,NuiHeight(NuiId(NuiButton(JsonString("Deutsch")),"de"),32.0));
    JsonArrayInsertInplace(rows,NuiHeight(NuiId(NuiButton(JsonString("Francais")),"fr"),32.0));
    JsonArrayInsertInplace(rows,NuiHeight(NuiId(NuiButton(JsonString("Espanol")),"es"),32.0));
    JsonArrayInsertInplace(rows,NuiHeight(NuiId(NuiButton(JsonString("Italiano")),"it"),32.0));
    return NuiWindow(NuiCol(rows),JsonString("Chat language"),NuiRect(-1.0,-1.0,260.0,290.0),JsonBool(FALSE),JsonBool(FALSE),JsonBool(TRUE),JsonBool(FALSE),JsonBool(TRUE));
}

void MgXlLanguageOpen(object pc)
{
    if(!GetLocalInt(GetModule(),"MGLIVE_XL") || !GetIsPC(pc)) return;
    int old=GetLocalInt(pc,"MGLIVE_LANG_TOKEN");
    if(old>0 && NuiGetWindowId(pc,old)=="mgliveLanguage") NuiDestroy(pc,old);
    int token=NuiCreate(pc,MgXlLanguageWindow(),"mgliveLanguage");
    SetLocalInt(pc,"MGLIVE_LANG_TOKEN",token);
}

void MgXlLanguageReply(json reply)
{
    object pc=StringToObject(JsonGetString(JsonObjectGet(reply,"pc")));
    string req=JsonGetString(JsonObjectGet(reply,"req"));
    string lang=JsonGetString(JsonObjectGet(reply,"lang"));
    if(!GetIsPC(pc) || req=="" || GetLocalString(pc,"MGLIVE_LANG_REQ")!=req || !MgXlKnownLanguage(lang)) return;
    DeleteLocalString(pc,"MGLIVE_LANG_REQ");
    SetLocalString(pc,"MGLIVE_LANG",lang);
    SendMessageToPC(pc,"Chat language: "+lang);
}

void MgXlLanguageInstall()
{
    object m=GetModule();
    string previous=GetEventScript(m,EVENT_SCRIPT_MODULE_ON_CLIENT_ENTER);
    if(previous=="mglive_xlent") return;
    SetLocalString(m,"MGLIVE_XL_ENTER_PREVIOUS",previous);
    SetEventScript(m,EVENT_SCRIPT_MODULE_ON_CLIENT_ENTER,"mglive_xlent");
}
