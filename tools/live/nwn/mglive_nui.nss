// Player-facing skill results. Uses the game's bundled NUI constructors.
#include "nw_inc_nui"

const string MGLIVE_SKILL_WINDOW = "mgliveSkill";

void MgLiveNuiInstall()
{
    object m = GetModule();
    string previous = GetEventScript(m, EVENT_SCRIPT_MODULE_ON_NUI_EVENT);
    if (previous == "mglive_nuievt") return;
    SetLocalString(m, "MGLIVE_NUI_PREVIOUS", previous);
    SetEventScript(m, EVENT_SCRIPT_MODULE_ON_NUI_EVENT, "mglive_nuievt");
}

string MgLiveSkillName(int skill)
{
    string name = Get2DAString("skills", "Name", skill);
    if (name != "" && name != "****") return GetStringByStrRef(StringToInt(name));
    return Get2DAString("skills", "Label", skill);
}

#include "mglive_skill"

const string MGLIVE_MESSAGE_WINDOW = "mgliveMessage";
json MgLiveMessageWindow(string text)
{
    json rows = JsonArrayInsert(JsonArray(), NuiHeight(NuiText(JsonString(text), FALSE),150.0));
    rows = JsonArrayInsert(rows,NuiHeight(NuiId(NuiButton(JsonString("OK")),"ok"),32.0));
    return NuiWindow(NuiCol(rows),JsonString("Message"),NuiRect(-1.0,-1.0,420.0,240.0),JsonBool(FALSE),JsonBool(FALSE),JsonBool(TRUE),JsonBool(FALSE),JsonBool(TRUE));
}
void MgLiveMessageClose(object pc,int token,int generation)
{
    if (GetIsObjectValid(pc) && GetLocalInt(pc,"MGLIVE_MESSAGE_GEN")==generation && NuiGetWindowId(pc,token)==MGLIVE_MESSAGE_WINDOW) NuiDestroy(pc,token);
}
int MgLiveShowMessage(object pc,string text)
{
    int token=NuiCreate(pc,MgLiveMessageWindow(text),MGLIVE_MESSAGE_WINDOW);
    int generation=GetLocalInt(pc,"MGLIVE_MESSAGE_GEN")+1;
    SetLocalInt(pc,"MGLIVE_MESSAGE_GEN",generation);
    if (token>0) AssignCommand(GetModule(),DelayCommand(30.0,MgLiveMessageClose(pc,token,generation)));
    else SendMessageToPC(pc,text);
    return token;
}
