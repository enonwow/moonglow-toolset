#include "mglive_nui"
#include "nwnx_creature"

void MgetSkillSnapshot(string key)
{
    json v=JsonObjectSet(JsonObject(),"base",JsonInt(GetSkillRank(SKILL_CONCENTRATION,OBJECT_SELF,TRUE)));
    v=JsonObjectSet(v,"total",JsonInt(GetSkillRank(SKILL_CONCENTRATION,OBJECT_SELF,FALSE)));
    v=JsonObjectSet(v,"ability",JsonInt(GetAbilityModifier(ABILITY_CONSTITUTION,OBJECT_SELF)));
    v=JsonObjectSet(v,"feat",JsonBool(GetHasFeat(FEAT_SKILL_FOCUS_CONCENTRATION,OBJECT_SELF)));
    v=JsonObjectSet(v,"ring",JsonBool(GetIsObjectValid(GetItemInSlot(INVENTORY_SLOT_LEFTRING,OBJECT_SELF))));
    SetLocalJson(GetModule(),key,v);
}

void main()
{
    object m=GetModule(); int stage=GetLocalInt(m,"MGET_ROLL_STAGE");
    if(stage==0)
    {
        NWNX_Creature_SetSkillRank(OBJECT_SELF,SKILL_CONCENTRATION,5);
        NWNX_Creature_SetRawAbilityScore(OBJECT_SELF,ABILITY_CONSTITUTION,18);
        if(GetHasFeat(FEAT_SKILL_FOCUS_CONCENTRATION))NWNX_Creature_RemoveFeat(OBJECT_SELF,FEAT_SKILL_FOCUS_CONCENTRATION);
        MgetSkillSnapshot("MGET_SKILL_BASE");
    }
    if(stage==1)
    {
        NWNX_Creature_AddFeat(OBJECT_SELF,FEAT_SKILL_FOCUS_CONCENTRATION);
        MgetSkillSnapshot("MGET_SKILL_FEAT");
    }
    if(stage==2)
    {
        object item=CreateItemOnObject("nw_it_mring001",OBJECT_SELF);
        AddItemProperty(DURATION_TYPE_PERMANENT,ItemPropertySkillBonus(SKILL_CONCENTRATION,4),item);
        ClearAllActions(TRUE); ActionEquipItem(item,INVENTORY_SLOT_LEFTRING);
        DelayCommand(1.0,MgetSkillSnapshot("MGET_SKILL_ITEM"));
    }
    if(stage==3)
    {
        ApplyEffectToObject(DURATION_TYPE_TEMPORARY,EffectSkillIncrease(SKILL_CONCENTRATION,5),OBJECT_SELF,60.0);
        MgetSkillSnapshot("MGET_SKILL_EFFECT");
    }
    if(stage==4 || stage==6 || stage==7)
    {
        SetLocalString(m,"MGET_SKILL_BREAKDOWN",MgLiveSkillBreakdown(SKILL_CONCENTRATION,OBJECT_SELF));
        string id="MGET_ROLL"; int timeout=60; int showDC=TRUE; int dc=15;
        if(stage==6){ id="MGET_NO_ROLL";timeout=2;showDC=FALSE; }
        if(stage==7){ id="MGET_FAIL";dc=255;showDC=FALSE; }
        json rows=JsonArrayInsert(JsonArray(),MgLiveSkillInvite(OBJECT_SELF,id,SKILL_CONCENTRATION,dc,2,showDC));
        json answer=MgLiveSkillBegin(id,rows,SKILL_CONCENTRATION,dc,2,showDC,timeout,"A clue.","",FALSE);
        SetLocalJson(m,"MGET_PROMPT",answer);
        SetLocalJson(m,"MGET_BUSY",MgLiveSkillInvite(OBJECT_SELF,"second",SKILL_CONCENTRATION,15,2,TRUE));
    }
    if(stage==5)
    {
        // Run after the enclosing run_script ack, as real NUI clicks do.
        DelayCommand(0.2,MgLiveSkillRoll(OBJECT_SELF));
        DelayCommand(0.3,MgLiveSkillRoll(OBJECT_SELF));
    }
}
