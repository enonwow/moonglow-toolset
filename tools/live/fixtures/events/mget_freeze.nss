void main()
{
    string mode = GetLocalString(GetModule(), "MGET_FREEZE_PROBE");
    if (mode == "apply")
        ApplyEffectToObject(DURATION_TYPE_TEMPORARY,
            TagEffect(EffectCutsceneParalyze(), "OTHER_FREEZE"), OBJECT_SELF, 60.0);
    if (mode == "remove")
    {
        effect e = GetFirstEffect(OBJECT_SELF);
        while (GetIsEffectValid(e))
        {
            if (GetEffectTag(e) == "OTHER_FREEZE") RemoveEffect(OBJECT_SELF, e);
            e = GetNextEffect(OBJECT_SELF);
        }
    }
    int own = 0; int other = 0;
    effect e = GetFirstEffect(OBJECT_SELF);
    while (GetIsEffectValid(e))
    {
        if (GetEffectTag(e) == "MGLIVE_FREEZE") own++;
        if (GetEffectTag(e) == "OTHER_FREEZE") other++;
        e = GetNextEffect(OBJECT_SELF);
    }
    SetName(OBJECT_SELF, "freeze=" + IntToString(own) + " other=" + IntToString(other));
}
