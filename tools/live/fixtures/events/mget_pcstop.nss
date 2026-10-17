#include "nwnx_creature"
#include "mglive_pcstop"

json ReadStopState(object actor, object victim)
{
    json j = JsonObjectSet(JsonObject(), "faction", JsonInt(NWNX_Creature_GetFaction(actor)));
    j = JsonObjectSet(j, "rep_actor", JsonInt(GetReputation(victim, actor)));
    j = JsonObjectSet(j, "rep_victim", JsonInt(GetReputation(actor, victim)));
    j = JsonObjectSet(j, "victim_faction", JsonInt(NWNX_Creature_GetFaction(victim)));
    j = JsonObjectSet(j, "attack_target", JsonBool(GetIsObjectValid(GetAttackTarget(actor))));
    j = JsonObjectSet(j, "combat", JsonBool(GetIsInCombat(actor)));
    j = JsonObjectSet(j, "action", JsonInt(GetCurrentAction(actor)));
    j = JsonObjectSet(j, "commandable", JsonBool(GetCommandable(actor)));
    j = JsonObjectSet(j, "held", JsonInt(GetLocalInt(actor, "MGLIVE_HELD")));
    return j;
}

void ObserveStop(object actor, object victim)
{
    SetLocalJson(GetModule(), "MGET_PCSTOP_AFTER", ReadStopState(actor, victim));
}

void main()
{
    object actor = GetLocalObject(GetModule(), "MGET_PCSTOP_ACTOR");
    object victim = GetLocalObject(GetModule(), "MGET_PCSTOP_VICTIM");
    // Defensive stale-bookkeeping probe: a PC stop must not restore an NPC
    // faction even if such a marker exists. The actor itself is still an NPC.
    SetLocalInt(actor, "MGLIVE_FACTION", 4);
    SetLocalJson(GetModule(), "MGET_PCSTOP_BEFORE", ReadStopState(actor, victim));
    MgLiveStopPlayerAttack(actor);
    DelayCommand(0.5, ObserveStop(actor, victim));
}
