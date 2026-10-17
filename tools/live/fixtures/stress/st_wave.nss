#include "nwnx_creature"
void main()
{
    object m=GetModule();int wave=GetLocalInt(m,"ST_WAVE")+1;SetLocalInt(m,"ST_WAVE",wave);
    int count=50;if(wave%3==0) count=128;if(wave%5==0) count=256;
    SetPlotFlag(OBJECT_SELF,FALSE);SetImmortal(OBJECT_SELF,TRUE);
    int i;for(i=0;i<count;i++) ApplyEffectToObject(DURATION_TYPE_INSTANT,EffectDamage(1,DAMAGE_TYPE_MAGICAL),OBJECT_SELF);
    SetLocalInt(m,"ST_DAMAGE",GetLocalInt(m,"ST_DAMAGE")+count);
    if(wave==1)
    {
        object a=GetObjectByTag("ST_3_40");object b=GetObjectByTag("ST_3_41");
        SetPlotFlag(a,FALSE);SetPlotFlag(b,FALSE);SetImmortal(a,TRUE);SetImmortal(b,TRUE);
        NWNX_Creature_SetFaction(a,1);NWNX_Creature_SetFaction(b,2);
        AssignCommand(a,ActionAttack(b));AssignCommand(b,ActionAttack(a));
    }
}
