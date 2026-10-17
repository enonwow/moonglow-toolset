#include "nwnx_events"
void MgetReleaseCast(int token)
{
    if (GetLocalInt(OBJECT_SELF, "MGET_CAST_TOKEN") == token) SetCommandable(TRUE);
}
void MgetHoldTarget()
{
    SetCommandable(TRUE);
    ClearAllActions(TRUE);
    SetCommandable(FALSE);
}
void main()
{
    WriteTimestampedLogEntry("MGET_CAST self=" + ObjectToString(OBJECT_SELF) + " type=" + IntToString(GetObjectType(OBJECT_SELF)) + " name=" + GetName(OBJECT_SELF));
    NWNX_Events_SubscribeEvent(NWNX_ON_SPELL_FAILED_AFTER, "mget_castfail");
    WriteTimestampedLogEntry("MGET_CAST ai=" + IntToString(GetAILevel(OBJECT_SELF)) + " commandable=" + IntToString(GetCommandable(OBJECT_SELF)));
    // Keep fixture AI explicit; changing this alone did not solve the cast failure.
    int token = GetLocalInt(OBJECT_SELF, "MGET_CAST_TOKEN") + 1;
    SetLocalInt(OBJECT_SELF, "MGET_CAST_TOKEN", token);
    SetAILevel(OBJECT_SELF, AI_LEVEL_VERY_HIGH);
    ClearAllActions(TRUE);
    object target = GetLocalObject(GetModule(), "MGET_TARGET");
    // Perception has been established by a real attack. Isolate the cast from
    // returning attacks/damage; an unheld opponent can cancel it mid-animation.
    AssignCommand(target, MgetHoldTarget());
    SetPlotFlag(OBJECT_SELF, TRUE);
    WriteTimestampedLogEntry("MGET_CAST target=" + ObjectToString(target) + " valid=" + IntToString(GetIsObjectValid(target)));
    WriteTimestampedLogEntry("MGET_CAST seen=" + IntToString(GetObjectSeen(target, OBJECT_SELF)));
    if (GetLocalInt(GetModule(), "MGET_CAST_LOCATION"))
        ActionCastSpellAtLocation(SPELL_FIREBALL, GetLocation(target),
            METAMAGIC_NONE, TRUE, PROJECTILE_PATH_TYPE_DEFAULT, FALSE);
    else
        ActionCastSpellAtObject(SPELL_MAGIC_MISSILE, target,
            METAMAGIC_NONE, TRUE, 0, PROJECTILE_PATH_TYPE_DEFAULT,
            !GetLocalInt(GetModule(), "MGET_CAST_PROJECTILE"));
    ActionDoCommand(SetCommandable(TRUE));
    SetCommandable(FALSE);
    DelayCommand(10.0, MgetReleaseCast(token));
    WriteTimestampedLogEntry("MGET_CAST action=" + IntToString(GetCurrentAction(OBJECT_SELF)));
}
