// Owned audit fixture. Native work is measured by the entry wrapper in prepare().
#include "mglive_inc"
#include "nwnx_chat"

void MpEquip(object actor, int base, int slot)
{
    json item = TemplateToJson("nw_it_mring001", RESTYPE_UTI);
    JsonObjectSetInplace(item, "BaseItem", JsonObjectSet(JsonObjectSet(JsonObject(), "type", JsonString("int")), "value", JsonInt(base)));
    object made = JsonToObject(item, GetLocation(actor), actor, FALSE);
    if (!GetIsObjectValid(made) || !NWNX_Creature_RunEquip(actor, made, slot))
        WriteTimestampedLogEntry("MP_SCALE_EQUIP_FAILED slot=" + IntToString(slot));
}

void main()
{
    object m = GetModule();
    int phase = GetLocalInt(m, "MP_SCALE_PHASE");
    object actor = OBJECT_SELF;
    if (phase == 0)
    {
        MpEquip(actor, BASE_ITEM_HELMET, INVENTORY_SLOT_HEAD);
        MpEquip(actor, BASE_ITEM_CLOAK, INVENTORY_SLOT_CLOAK);
        MpEquip(actor, BASE_ITEM_SMALLSHIELD, INVENTORY_SLOT_LEFTHAND);
        int i;
        for (i = 0; i < 300; i++) SetLocalString(actor, "scale_local_" + IntToString(i), "bounded native local fixture");
        SetLocalInt(actor, "MP_LOOK", TRUE);
    }
    else if (phase == 1)
    {
        int i;
        SetPlotFlag(actor, FALSE);
        SetImmortal(actor, TRUE);
        // Actual Damage plugin callbacks, not manually fabricated event rows.
        for (i = 0; i < 256; i++)
            ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectDamage(1, DAMAGE_TYPE_MAGICAL), actor);
        SetLocalInt(m, "MP_SCALE_BUFFERED", GetLocalInt(m, "MGLIVE_EVENT_COUNT"));
        SetLocalInt(m, "MGLIVE_RESYNC", TRUE);
    }
    else if (phase == 2)
    {
        NWNX_Chat_SendMessage(1, "Full scale native speech", actor);
        AssignCommand(actor, ActionCastSpellAtObject(SPELL_MAGIC_MISSILE, actor, METAMAGIC_ANY, TRUE));
    }
}
