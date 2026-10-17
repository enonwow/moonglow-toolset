// Native fixture; does not synthesize event callbacks or wire data.
void main()
{
    int stage = GetLocalInt(GetModule(), "MGET_VFX_STAGE");
    effect e = GetFirstEffect(OBJECT_SELF);
    while (GetIsEffectValid(e))
    {
        if (GetEffectType(e, TRUE) == EFFECT_TYPE_VISUALEFFECT)
            RemoveEffect(OBJECT_SELF, e);
        e = GetNextEffect(OBJECT_SELF);
    }
    if (stage == 0)
    {
        ApplyEffectToObject(DURATION_TYPE_PERMANENT, EffectVisualEffect(VFX_DUR_GLOW_RED), OBJECT_SELF);
        e = GetFirstEffect(OBJECT_SELF);
        while (GetIsEffectValid(e))
        {
            if (GetEffectType(e, TRUE) == EFFECT_TYPE_VISUALEFFECT)
            {
                string probe = "vfx-probe";
                int i;
                for (i = 0; i < 8; i++) probe += " " + IntToString(GetEffectInteger(e, i));
                SetName(OBJECT_SELF, probe);
            }
            e = GetNextEffect(OBJECT_SELF);
        }
    }
    if (stage == 1)
    {
        ApplyEffectToObject(DURATION_TYPE_TEMPORARY, EffectVisualEffect(VFX_DUR_GLOW_BLUE), OBJECT_SELF, 60.0);
        ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectVisualEffect(VFX_IMP_HEALING_M), OBJECT_SELF);
        SetName(OBJECT_SELF, "vfx-temporary " + IntToString(VFX_DUR_GLOW_BLUE));
    }
    if (stage == 3)
    {
        int n;
        for (n = 0; n < 17; n++)
            ApplyEffectToObject(DURATION_TYPE_PERMANENT,
                TagEffect(EffectVisualEffect(VFX_DUR_GLOW_RED), "mget-vfx-" + IntToString(n)), OBJECT_SELF);
        int count = 0;
        e = GetFirstEffect(OBJECT_SELF);
        while (GetIsEffectValid(e))
        {
            if (GetEffectType(e, TRUE) == EFFECT_TYPE_VISUALEFFECT) count++;
            e = GetNextEffect(OBJECT_SELF);
        }
        SetName(OBJECT_SELF, "vfx-count " + IntToString(count));
    }
}
