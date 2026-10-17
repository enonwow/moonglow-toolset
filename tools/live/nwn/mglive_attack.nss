#include "mglive_evt"
#include "nwnx_damage"
void main()
{
    struct NWNX_Damage_AttackEventData d = NWNX_Damage_GetAttackEventData();
    json e = MgLiveEvent("attack", OBJECT_SELF, d.oTarget);
    e = JsonObjectSet(e, "roll", JsonInt(d.iToHitRoll));
    e = JsonObjectSet(e, "modifier", JsonInt(d.iToHitModifier));
    e = JsonObjectSet(e, "result", JsonInt(d.iAttackResult));
    e = JsonObjectSet(e, "attack_number", JsonInt(d.iAttackNumber));
    e = JsonObjectSet(e, "attack_type", JsonInt(d.iAttackType));
    e = JsonObjectSet(e, "weapon_attack_type", JsonInt(d.iWeaponAttackType));
    e = JsonObjectSet(e, "ranged", JsonBool(d.bRangedAttack));
    e = JsonObjectSet(e, "killing_blow", JsonBool(d.bKillingBlow));
    MgLiveQueueEvent(e);
}
