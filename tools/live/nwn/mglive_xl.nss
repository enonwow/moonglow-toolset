// Opt-in feasibility prototype. All pending state and the fallback live in game.
#include "nwnx_chat"
#include "nwnx_redis"
#include "mglive_text"

int MgXlPlayer(object o) { return GetIsPC(o) || GetIsDM(o) || GetIsDMPossessed(o); }
int MgXlDM(object o) { return GetIsDM(o) || GetIsDMPossessed(o); }
json MgXlClients()
{
    json clients=JsonArray(); object pc=GetFirstPC();
    while(GetIsObjectValid(pc))
    {
        JsonArrayInsertInplace(clients,JsonString(ObjectToString(pc)));
        if(JsonGetLength(clients)>128) return clients;
        pc=GetNextPC();
    }
    return clients;
}

string MgXlLanguage(object pc)
{
    string lang=GetStringLowerCase(GetLocalString(pc,"MGLIVE_LANG"));
    if(lang=="" || GetStringLength(lang)>16) return "en";
    int i; for(i=0;i<GetStringLength(lang);i++)
        if(FindSubString("abcdefghijklmnopqrstuvwxyz-",GetSubString(lang,i,1))<0) return "en";
    return lang;
}

// PCs are selected at speech time, never from a later report or reply.
int MgXlHears(int channel, object speaker, object target, object listener)
{
    if(!GetIsObjectValid(listener)) return FALSE;
    if(channel==4 || channel==20) return listener==target || listener==speaker;
    if(channel==14 || channel==30) return MgXlDM(listener) || listener==speaker;
    if(channel==6 || channel==22) return GetFactionEqual(speaker,listener);
    if(channel==2 || channel==18) return TRUE;
    if(channel==1 || channel==3 || channel==17 || channel==19)
    {
        float range=NWNX_Chat_GetChatHearingDistance(listener,channel);
        return GetArea(speaker)==GetArea(listener) && GetDistanceBetween(speaker,listener)<=range;
    }
    return FALSE;
}

// Preserve the exact sequence of colour/other tags and emote delimiters.
string MgXlMarkup(string text)
{
    string out=""; int i; int n=GetStringLength(text);
    for(i=0;i<n;i++)
    {
        string c=GetSubString(text,i,1);
        if(c=="*") out+="*";
        if(c=="<")
        {
            int end=FindSubString(text,">",i);
            if(end<0) return "INVALID";
            out+=GetSubString(text,i,end-i+1);i=end;
        }
        else if(c==">") return "INVALID";
    }
    return out;
}

int MgXlCommand(string text)
{
    int i; string c;
    for(i=0;i<GetStringLength(text);i++)
    {
        c=GetSubString(text,i,1);
        if(c!=" " && c!=GetSubString(NWNX_Util_GetAsciiTableString(),9,1) &&
           c!=GetSubString(NWNX_Util_GetAsciiTableString(),13,1) && c!="\n") break;
    }
    string prefixes="/!"+GetLocalString(GetModule(),"MGLIVE_XL_COMMANDS");
    return c!="" && FindSubString(prefixes,c)>=0;
}

int MgXlSafe(string original, string translated)
{
    int n=GetStringLength(translated);
    if(n==0 || n>1024 || n>GetStringLength(original)*4+32 || MgXlCommand(translated)) return FALSE;
    // NWScript does not implement the C carriage-return escape (it becomes "r").
    if(FindSubString(translated,"\n")>=0 ||
       FindSubString(translated,GetSubString(NWNX_Util_GetAsciiTableString(),13,1))>=0) return FALSE;
    string marks=MgXlMarkup(original);
    return marks!="INVALID" && marks==MgXlMarkup(translated);
}

void MgXlSend(int channel,string text,object speaker,object listener)
{
    object m=GetModule();
    // Tells and untargeted sends re-enter NWNX's hook, even from pub/sub.
    SetLocalInt(m,"MGLIVE_XL_DELIVERING",TRUE);
    NWNX_Chat_SendMessage(channel,text,speaker,listener);
    DeleteLocalInt(m,"MGLIVE_XL_DELIVERING");
}

void MgXlFinish(int slot,string req,json translations)
{
    object m=GetModule();string key="MGLIVE_XL_"+IntToString(slot);
    json pending=GetLocalJson(m,key);
    if(JsonGetString(JsonObjectGet(pending,"req"))!=req) return;
    // Consume before sending: a duplicate, late reply or fallback cannot repeat it.
    DeleteLocalJson(m,key);
    int elapsed=GetMicrosecondCounter()-JsonGetInt(JsonObjectGet(pending,"at"));
    if(elapsed>=900000 || elapsed<0) translations=JsonObject();
    object speaker=StringToObject(JsonGetString(JsonObjectGet(pending,"speaker")));
    string original=JsonGetString(JsonObjectGet(pending,"text"));
    int channel=JsonGetInt(JsonObjectGet(pending,"channel"));
    json listeners=JsonObjectGet(pending,"listeners");int i;
    for(i=0;i<JsonGetLength(listeners);i++)
    {
        json row=JsonArrayGet(listeners,i);
        object listener=StringToObject(JsonGetString(JsonObjectGet(row,"id")));
        string lang=JsonGetString(JsonObjectGet(row,"lang"));
        string text=JsonGetString(JsonObjectGet(translations,lang));
        if(!MgXlSafe(original,text)) text=original;
        if(GetIsObjectValid(listener)) MgXlSend(channel,text,speaker,listener);
    }
}

void MgXlReply(string raw)
{
    if(GetStringLength(raw)>16384) return;
    json reply=JsonParse(raw);
    int slot=JsonGetInt(JsonObjectGet(reply,"slot"));
    if(slot<0 || slot>=128) return;
    string req=JsonGetString(JsonObjectGet(reply,"req"));
    if(req=="") return;
    MgXlFinish(slot,req,JsonObjectGet(reply,"to"));
}

int MgXlCapture(int channel,string text,object speaker,object target)
{
    object m=GetModule();
    if(!GetLocalInt(m,"MGLIVE_XL") || GetLocalInt(m,"MGLIVE_XL_DELIVERING")) return FALSE;
    if(!GetIsObjectValid(speaker) || GetStringLength(text)>1024 || MgXlCommand(text)) return FALSE;
    if(channel!=1 && channel!=2 && channel!=3 && channel!=4 && channel!=6 &&
       channel!=14 && channel!=17 && channel!=18 && channel!=19 && channel!=20 && channel!=22 && channel!=30) return FALSE;
    // Silent NPC channel 14 belongs to AI, not player-to-DM speech.
    if(channel==14 && !MgXlPlayer(speaker)) return FALSE;
    if((channel==4 || channel==20) && !GetIsObjectValid(target)) return FALSE;
    int sequence=GetLocalInt(m,"MGLIVE_XL_SEQ")+1;
    if(sequence<=0) sequence=1;
    int slot=sequence%128;string key="MGLIVE_XL_"+IntToString(slot);
    if(JsonGetType(GetLocalJson(m,key))==JSON_TYPE_OBJECT) return FALSE;
    json clients=MgXlClients();
    if(JsonGetLength(clients)>128) return FALSE;
    json immediate=JsonArray();json held=JsonArray();json languages=JsonObject();
    string source=MgXlLanguage(speaker);int i;
    for(i=0;i<JsonGetLength(clients);i++)
    {
        object pc=StringToObject(JsonGetString(JsonArrayGet(clients,i)));
        if(MgXlHears(channel,speaker,target,pc))
        {
            string lang=MgXlLanguage(pc);
            if(lang==source || pc==speaker) JsonArrayInsertInplace(immediate,JsonString(ObjectToString(pc)));
            else
            {
                json row=JsonObjectSet(JsonObject(),"id",JsonString(ObjectToString(pc)));
                JsonArrayInsertInplace(held,JsonObjectSet(row,"lang",JsonString(lang)));
                JsonObjectSetInplace(languages,lang,JsonBool(TRUE));
            }
        }
    }
    if(JsonGetLength(held)==0) return FALSE; // preserve native delivery with no translation work
    // Preserve listening NPCs for public speech. They hear original text, never translated commands.
    if(channel==1 || channel==2 || channel==3 || channel==17 || channel==18 || channel==19)
    {
        object area=GetArea(speaker);object npc=GetFirstObjectInArea(area);
        while(GetIsObjectValid(npc))
        {
            if(npc!=speaker && !MgXlPlayer(npc) && GetIsListening(npc) && MgXlHears(channel,speaker,target,npc))
                JsonArrayInsertInplace(immediate,JsonString(ObjectToString(npc)));
            if(JsonGetLength(immediate)>256) return FALSE;
            npc=GetNextObjectInArea(area);
        }
    }
    string req=GetLocalString(m,"MGLIVE_BOOT")+":"+IntToString(sequence);
    json pending=JsonObjectSet(JsonObject(),"req",JsonString(req));
    JsonObjectSetInplace(pending,"slot",JsonInt(slot));
    JsonObjectSetInplace(pending,"at",JsonInt(GetMicrosecondCounter()));
    JsonObjectSetInplace(pending,"speaker",JsonString(ObjectToString(speaker)));
    JsonObjectSetInplace(pending,"text",JsonString(text));
    JsonObjectSetInplace(pending,"channel",JsonInt(channel));
    JsonObjectSetInplace(pending,"listeners",held);
    SetLocalJson(m,key,pending);SetLocalInt(m,"MGLIVE_XL_SEQ",sequence);
    AssignCommand(m,DelayCommand(0.9,MgXlFinish(slot,req,JsonObject())));
    for(i=0;i<JsonGetLength(immediate);i++)
        MgXlSend(channel,text,speaker,StringToObject(JsonGetString(JsonArrayGet(immediate,i))));
    json request=MgLiveTextField(pending,"text",text);
    JsonObjectDelInplace(request,"listeners");
    JsonObjectSetInplace(request,"from",JsonString(source));
    JsonObjectSetInplace(request,"to",languages);
    JsonObjectSetInplace(request,"text_codepage",JsonString(MgLiveTextCodepage()));
    NWNX_Redis_PUBLISH(GetLocalString(m,"MGLIVE_PREFIX")+".xl.req",JsonDump(request));
    return TRUE;
}
