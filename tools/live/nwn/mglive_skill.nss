// Interactive rolls: server-owned inputs, one roll per recipient, bounded replies.
// Included by mglive_nui after its common window helpers and skill-name lookup.
#include "mglive_evt"
#include "mglive_result"

string MgLiveSkillSigned(int value)
{
    if(value>=0) return "+"+IntToString(value);
    return IntToString(value);
}

// GetSkillRank(TRUE) is purchased ranks; FALSE includes ability, feats, item
// and active-effect bonuses. Other bonuses are aggregated, never guessed.
string MgLiveSkillBreakdown(int skill, object pc)
{
    int ranks=GetSkillRank(skill,pc,TRUE); int total=GetSkillRank(skill,pc,FALSE);
    string key=Get2DAString("skills","KeyAbility",skill); int ability=-1;
    if(key=="STR")ability=ABILITY_STRENGTH;
    if(key=="DEX")ability=ABILITY_DEXTERITY;
    if(key=="CON")ability=ABILITY_CONSTITUTION;
    if(key=="INT")ability=ABILITY_INTELLIGENCE;
    if(key=="WIS")ability=ABILITY_WISDOM;
    if(key=="CHA")ability=ABILITY_CHARISMA;
    int modifier=0; string line="Ranks "+IntToString(ranks);
    if(ability>=0)
    {
        modifier=GetAbilityModifier(ability,pc);
        line+="  |  "+key+" "+MgLiveSkillSigned(modifier);
    }
    return line+"  |  Other "+MgLiveSkillSigned(total-ranks-modifier)+"  =  "+MgLiveSkillSigned(total);
}

json MgLiveSkillWindow(int skill, int dc, int bonus, int showDC, object pc=OBJECT_INVALID)
{
    string dcLine="DC hidden"; if(showDC) dcLine="DC "+IntToString(dc);
    json rows=JsonArrayInsert(JsonArray(),NuiHeight(NuiLabel(JsonString(dcLine),JsonInt(NUI_HALIGN_CENTER),JsonInt(NUI_VALIGN_MIDDLE)),26.0));
    rows=JsonArrayInsert(rows,NuiHeight(NuiLabel(NuiBind("breakdown"),JsonInt(NUI_HALIGN_CENTER),JsonInt(NUI_VALIGN_MIDDLE)),30.0));
    if(bonus!=0) rows=JsonArrayInsert(rows,NuiHeight(NuiLabel(JsonString(MgLiveSkillSigned(bonus)+" DM bonus"),JsonInt(NUI_HALIGN_CENTER),JsonInt(NUI_VALIGN_MIDDLE)),24.0));
    rows=JsonArrayInsert(rows,NuiHeight(NuiVisible(NuiId(NuiButton(JsonString("Roll")),"roll"),NuiBind("can_roll")),52.0));
    rows=JsonArrayInsert(rows,NuiHeight(NuiVisible(NuiStyleForegroundColor(NuiLabel(NuiBind("outcome"),JsonInt(NUI_HALIGN_CENTER),JsonInt(NUI_VALIGN_MIDDLE)),NuiBind("outcome_color")),NuiBind("finished")),38.0));
    rows=JsonArrayInsert(rows,NuiHeight(NuiVisible(NuiText(NuiBind("message"),FALSE),NuiBind("has_message")),64.0));
    rows = JsonArrayInsert(rows, NuiHeight(NuiId(NuiButton(JsonString("OK")), "ok"), 32.0));
    float height=300.0; if(bonus!=0)height+=24.0;
    return NuiWindow(NuiCol(rows), JsonString(MgLiveSkillName(skill)+" check"), NuiRect(-1.0, -1.0, 520.0, height),
        JsonBool(FALSE), JsonBool(FALSE), JsonBool(TRUE), JsonBool(FALSE), JsonBool(TRUE));
}

void MgLiveSkillClose(object pc, int token, int generation)
{
    if (GetIsObjectValid(pc) && GetLocalInt(pc,"MGLIVE_SKILL_TOKEN") == token &&
        GetLocalInt(pc,"MGLIVE_SKILL_GEN") == generation &&
        NuiGetWindowId(pc, token) == MGLIVE_SKILL_WINDOW) NuiDestroy(pc, token);
}

json MgLiveSkillAnswer(string id, json value)
{
    json rows = JsonObjectGet(value,"players"); int i; int pending = FALSE;
    for (i=0; i<JsonGetLength(rows); i++)
        if (JsonGetString(JsonObjectGet(JsonArrayGet(rows,i),"status")) == "waiting") pending=TRUE;
    json answer = JsonObjectSet(MgLiveResult(id),"value",value);
    answer = JsonObjectSet(answer,"text_kind",JsonString("skill"));
    string by=GetLocalString(GetModule(),"MGLIVE_SKILL_BY_"+id);if(by!="") answer=MgLiveTextField(answer,"by",by);
    return JsonObjectSet(answer,"pending",JsonBool(pending));
}

void MgLiveSkillPublish(string id, json value)
{
    object m = GetModule();
    json answer = MgLiveSkillAnswer(id,value);
    json results = GetLocalJson(m,"MGLIVE_RESULTS"); int i;
    for(i=JsonGetLength(results)-1;i>=0;i--)
        if(JsonGetString(JsonObjectGet(JsonArrayGet(results,i),"id"))==id) results=JsonArrayDel(results,i);
    SetLocalJson(m,"MGLIVE_RESULTS",JsonArrayInsert(results,answer));
    json ack = GetLocalJson(m,"MGLIVE_ACK");
    if(JsonGetType(JsonFind(ack,JsonString(id)))==JSON_TYPE_NULL) SetLocalJson(m,"MGLIVE_ACK",JsonArrayInsert(ack,JsonString(id)));
    if (JsonGetInt(JsonObjectGet(answer,"pending"))) SetLocalJson(m,"MGLIVE_SKILL_"+id,value);
    else
    {
        DeleteLocalJson(m,"MGLIVE_SKILL_"+id);
        DeleteLocalString(m,"MGLIVE_SKILL_YES_"+id);
        DeleteLocalString(m,"MGLIVE_SKILL_NO_"+id);
        DeleteLocalString(m,"MGLIVE_SKILL_BY_"+id);
        SetLocalInt(m,"MGLIVE_SKILL_ACTIVE",GetLocalInt(m,"MGLIVE_SKILL_ACTIVE")-1);
    }
}

void MgLiveSkillTimeout(string id)
{
    object m=GetModule(); json value=GetLocalJson(m,"MGLIVE_SKILL_"+id);
    if(JsonGetType(value)!=JSON_TYPE_OBJECT) return;
    json rows=JsonObjectGet(value,"players"); int i;
    for(i=0;i<JsonGetLength(rows);i++)
    {
        json row=JsonArrayGet(rows,i);
        if(JsonGetString(JsonObjectGet(row,"status"))!="waiting") continue;
        string oid=JsonGetString(JsonObjectGet(row,"id")); object pc=StringToObject(oid);
        row=JsonObjectSet(row,"status",JsonString("no_roll"));
        rows=JsonArraySet(rows,i,row);
        json e=MgLiveEvent("skill_roll",pc); e=JsonObjectSet(e,"request",JsonString(id));
        e=JsonObjectSet(e,"command_id",JsonString(id));
        string by=GetLocalString(m,"MGLIVE_SKILL_BY_"+id);if(by!="") e=MgLiveTextField(e,"by",by);
        e=JsonObjectSet(e,"id",JsonString(oid)); e=JsonObjectSet(e,"status",JsonString("no_roll"));
        MgLiveQueueEvent(e);
        if(GetIsObjectValid(pc) && GetLocalString(pc,"MGLIVE_SKILL_REQUEST")==id)
        {
            DeleteLocalString(pc,"MGLIVE_SKILL_REQUEST");
            int token=GetLocalInt(pc,"MGLIVE_SKILL_TOKEN");
            if(token>0 && NuiGetWindowId(pc,token)==MGLIVE_SKILL_WINDOW)
            {
                NuiSetBind(pc,token,"can_roll",JsonBool(FALSE));
                NuiSetBind(pc,token,"outcome",JsonString("No roll: time expired."));
                NuiSetBind(pc,token,"finished",JsonBool(TRUE));
                NuiSetBind(pc,token,"outcome_color",NuiColor(205,205,215));
                int generation=GetLocalInt(pc,"MGLIVE_SKILL_GEN");
                AssignCommand(m,DelayCommand(30.0,MgLiveSkillClose(pc,token,generation)));
            }
            SendMessageToPC(pc,MgLiveSkillName(JsonGetInt(JsonObjectGet(value,"skill")))+": no roll (time expired).");
        }
    }
    MgLiveSkillPublish(id,JsonObjectSet(value,"players",rows));
}

// Called only after the NUI event router verifies the player's own window/token.
// Kept separate so a headless fixture can prove the exact calculation on an NPC.
void MgLiveSkillRoll(object pc)
{
    if(!GetIsObjectValid(pc) || GetIsDead(pc)) return;
    string id=GetLocalString(pc,"MGLIVE_SKILL_REQUEST");
    object m=GetModule(); json value=GetLocalJson(m,"MGLIVE_SKILL_"+id);
    if(id=="" || JsonGetType(value)!=JSON_TYPE_OBJECT) return;
    json rows=JsonObjectGet(value,"players"); int i;
    for(i=0;i<JsonGetLength(rows);i++)
    {
        json row=JsonArrayGet(rows,i);
        if(JsonGetString(JsonObjectGet(row,"id"))!=ObjectToString(pc) || JsonGetString(JsonObjectGet(row,"status"))!="waiting") continue;
        int skill=JsonGetInt(JsonObjectGet(value,"skill")); int dc=JsonGetInt(JsonObjectGet(value,"dc"));
        int bonus=JsonGetInt(JsonObjectGet(value,"bonus")); int modifier=GetSkillRank(skill,pc,FALSE);
        int roll=d20(); int total=roll+modifier+bonus; int passed=total>=dc;
        row=JsonObjectSet(row,"status",JsonString("rolled"));
        row=JsonObjectSet(row,"roll",JsonInt(roll)); row=JsonObjectSet(row,"modifier",JsonInt(modifier));
        row=JsonObjectSet(row,"total",JsonInt(total)); row=JsonObjectSet(row,"passed",JsonBool(passed));
        rows=JsonArraySet(rows,i,row);
        string outcome="failure"; if(passed)outcome="success";
        string line=MgLiveSkillName(skill)+": "+IntToString(roll)+" + "+IntToString(modifier)+" + "+IntToString(bonus)+" (DM) = "+IntToString(total);
        if(JsonGetInt(JsonObjectGet(value,"show_dc"))) line+=" vs DC "+IntToString(dc);
        line+=": "+outcome;
        string message=GetLocalString(m,"MGLIVE_SKILL_NO_"+id);
        if(passed)message=GetLocalString(m,"MGLIVE_SKILL_YES_"+id);
        SendMessageToPC(pc,line);
        if(message!="")SendMessageToPC(pc,message);
        int token=GetLocalInt(pc,"MGLIVE_SKILL_TOKEN");
        if(token>0 && NuiGetWindowId(pc,token)==MGLIVE_SKILL_WINDOW)
        {
            NuiSetBind(pc,token,"can_roll",JsonBool(FALSE));
            NuiSetBind(pc,token,"breakdown",JsonString(MgLiveSkillBreakdown(skill,pc)));
            string display="d20 "+IntToString(roll)+" "+MgLiveSkillSigned(modifier);
            if(bonus!=0)display+=" "+MgLiveSkillSigned(bonus)+" DM";
            display+=" = "+IntToString(total)+": "+outcome;
            NuiSetBind(pc,token,"outcome",JsonString(display));
            NuiSetBind(pc,token,"finished",JsonBool(TRUE));
            json color=NuiColor(245,100,100); if(passed)color=NuiColor(115,225,145);
            NuiSetBind(pc,token,"outcome_color",color);
            NuiSetBind(pc,token,"message",JsonString(message));
            NuiSetBind(pc,token,"has_message",JsonBool(message!=""));
            int generation=GetLocalInt(pc,"MGLIVE_SKILL_GEN");
            AssignCommand(m,DelayCommand(30.0,MgLiveSkillClose(pc,token,generation)));
        }
        json e=MgLiveEvent("skill_roll",pc); e=JsonObjectSet(e,"request",JsonString(id));
        e=JsonObjectSet(e,"command_id",JsonString(id));
        string by=GetLocalString(m,"MGLIVE_SKILL_BY_"+id);if(by!="") e=MgLiveTextField(e,"by",by);
        e=JsonObjectSet(e,"skill",JsonInt(skill)); e=JsonObjectSet(e,"dc",JsonInt(dc));
        e=JsonObjectSet(e,"bonus",JsonInt(bonus)); e=JsonObjectSet(e,"roll",JsonInt(roll));
        e=JsonObjectSet(e,"modifier",JsonInt(modifier)); e=JsonObjectSet(e,"total",JsonInt(total));
        e=JsonObjectSet(e,"passed",JsonBool(passed)); e=JsonObjectSet(e,"status",JsonString("rolled"));
        e=MgLiveTextField(e,"text",line); MgLiveQueueEvent(e);
        DeleteLocalString(pc,"MGLIVE_SKILL_REQUEST");
        MgLiveSkillPublish(id,JsonObjectSet(value,"players",rows));
        return; // A duplicate click cannot roll again.
    }
}

json MgLiveSkillInvite(object pc, string id, int skill, int dc, int bonus, int showDC)
{
    json row=JsonObjectSet(JsonObject(),"id",JsonString(ObjectToString(pc)));
    if(GetLocalString(pc,"MGLIVE_SKILL_REQUEST")!="") return JsonObjectSet(row,"status",JsonString("busy"));
    int old=GetLocalInt(pc,"MGLIVE_SKILL_TOKEN");
    int generation=GetLocalInt(pc,"MGLIVE_SKILL_GEN");
    MgLiveSkillClose(pc,old,generation);
    SetLocalInt(pc,"MGLIVE_SKILL_GEN",generation+1);
    int token=NuiCreate(pc,MgLiveSkillWindow(skill,dc,bonus,showDC,pc),MGLIVE_SKILL_WINDOW);
    SetLocalString(pc,"MGLIVE_SKILL_REQUEST",id); SetLocalInt(pc,"MGLIVE_SKILL_TOKEN",token);
    if(token>0)
    {
        NuiSetBind(pc,token,"can_roll",JsonBool(TRUE));
        NuiSetBind(pc,token,"breakdown",JsonString(MgLiveSkillBreakdown(skill,pc)));
        NuiSetBind(pc,token,"outcome",JsonString(""));
        NuiSetBind(pc,token,"outcome_color",NuiColor(205,205,215));
        NuiSetBind(pc,token,"finished",JsonBool(FALSE));
        NuiSetBind(pc,token,"message",JsonString(""));
        NuiSetBind(pc,token,"has_message",JsonBool(FALSE));
    }
    else SendMessageToPC(pc,MgLiveSkillName(skill)+": skill check requested; NUI unavailable. No automatic roll; this request will time out.");
    row=JsonObjectSet(row,"nui_token",JsonInt(token));
    return JsonObjectSet(row,"status",JsonString("waiting"));
}

json MgLiveSkillBegin(string id, json rows, int skill, int dc, int bonus, int showDC, int timeout, string yes, string no, int truncated)
{
    object m=GetModule(); json value=JsonObjectSet(JsonObject(),"players",rows);
    SetLocalString(m,"MGLIVE_SKILL_BY_"+id,GetLocalString(m,"MGLIVE_COMMAND_BY"));
    value=MgLiveTextField(value,"name",GetStringLeft(MgLiveSkillName(skill),80));
    value=JsonObjectSet(value,"skill",JsonInt(skill)); value=JsonObjectSet(value,"dc",JsonInt(dc));
    value=JsonObjectSet(value,"bonus",JsonInt(bonus)); value=JsonObjectSet(value,"show_dc",JsonBool(showDC));
    value=JsonObjectSet(value,"timeout",JsonInt(timeout)); value=JsonObjectSet(value,"truncated",JsonBool(truncated));
    json result=MgLiveSkillAnswer(id,value);
    if(JsonGetInt(JsonObjectGet(result,"pending")))
    {
        SetLocalJson(m,"MGLIVE_SKILL_"+id,value);
        SetLocalString(m,"MGLIVE_SKILL_YES_"+id,yes); SetLocalString(m,"MGLIVE_SKILL_NO_"+id,no);
        SetLocalInt(m,"MGLIVE_SKILL_ACTIVE",GetLocalInt(m,"MGLIVE_SKILL_ACTIVE")+1);
        AssignCommand(m,DelayCommand(IntToFloat(timeout),MgLiveSkillTimeout(id)));
    }
    if(!JsonGetInt(JsonObjectGet(result,"pending"))) DeleteLocalString(m,"MGLIVE_SKILL_BY_"+id);
    return result;
}
