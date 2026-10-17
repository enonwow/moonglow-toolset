// Shared by the guarded PC command and the compiled headless NPC stand-in.
// Do not use MgLiveCancelHold here: its faction restoration is for NPC orders.
void MgLiveStopPlayerAttack(object actor)
{
    DeleteLocalInt(actor, "MGLIVE_HELD");
    DeleteLocalInt(actor, "MGLIVE_FACTION");
    DeleteLocalInt(actor, "MGLIVE_PATROL_ACTIVE");
    DeleteLocalJson(actor, "MGLIVE_PATROL_POINTS");
    // DM force: release our uncommandable attack/movement hold and leave the
    // player free to give another order. This is no temporary friendship/truce.
    SetCommandable(TRUE, actor);
    AssignCommand(actor, ClearAllActions(TRUE));
}
