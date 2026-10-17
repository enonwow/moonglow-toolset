#include "mglive_evt"
#include "nwnx_damage"
void main()
{
    struct NWNX_Damage_DamageEventData d = NWNX_Damage_GetDamageEventData();
    json e = MgLiveEvent("damage", d.oDamager, OBJECT_SELF);
    e = JsonObjectSet(e, "area", JsonString(GetResRef(GetArea(OBJECT_SELF))));
    json amounts = JsonObject();
    int total = 0;
    if (d.iBludgeoning > 0) { amounts = JsonObjectSet(amounts, "bludgeoning", JsonInt(d.iBludgeoning)); total += d.iBludgeoning; }
    if (d.iPierce > 0) { amounts = JsonObjectSet(amounts, "piercing", JsonInt(d.iPierce)); total += d.iPierce; }
    if (d.iSlash > 0) { amounts = JsonObjectSet(amounts, "slashing", JsonInt(d.iSlash)); total += d.iSlash; }
    if (d.iMagical > 0) { amounts = JsonObjectSet(amounts, "magical", JsonInt(d.iMagical)); total += d.iMagical; }
    if (d.iAcid > 0) { amounts = JsonObjectSet(amounts, "acid", JsonInt(d.iAcid)); total += d.iAcid; }
    if (d.iCold > 0) { amounts = JsonObjectSet(amounts, "cold", JsonInt(d.iCold)); total += d.iCold; }
    if (d.iDivine > 0) { amounts = JsonObjectSet(amounts, "divine", JsonInt(d.iDivine)); total += d.iDivine; }
    if (d.iElectrical > 0) { amounts = JsonObjectSet(amounts, "electrical", JsonInt(d.iElectrical)); total += d.iElectrical; }
    if (d.iFire > 0) { amounts = JsonObjectSet(amounts, "fire", JsonInt(d.iFire)); total += d.iFire; }
    if (d.iNegative > 0) { amounts = JsonObjectSet(amounts, "negative", JsonInt(d.iNegative)); total += d.iNegative; }
    if (d.iPositive > 0) { amounts = JsonObjectSet(amounts, "positive", JsonInt(d.iPositive)); total += d.iPositive; }
    if (d.iSonic > 0) { amounts = JsonObjectSet(amounts, "sonic", JsonInt(d.iSonic)); total += d.iSonic; }
    if (d.iBase > 0) { amounts = JsonObjectSet(amounts, "base", JsonInt(d.iBase)); total += d.iBase; }
    if (d.iCustom1 > 0) { amounts = JsonObjectSet(amounts, "custom1", JsonInt(d.iCustom1)); total += d.iCustom1; }
    if (d.iCustom2 > 0) { amounts = JsonObjectSet(amounts, "custom2", JsonInt(d.iCustom2)); total += d.iCustom2; }
    if (d.iCustom3 > 0) { amounts = JsonObjectSet(amounts, "custom3", JsonInt(d.iCustom3)); total += d.iCustom3; }
    if (d.iCustom4 > 0) { amounts = JsonObjectSet(amounts, "custom4", JsonInt(d.iCustom4)); total += d.iCustom4; }
    if (d.iCustom5 > 0) { amounts = JsonObjectSet(amounts, "custom5", JsonInt(d.iCustom5)); total += d.iCustom5; }
    if (d.iCustom6 > 0) { amounts = JsonObjectSet(amounts, "custom6", JsonInt(d.iCustom6)); total += d.iCustom6; }
    if (d.iCustom7 > 0) { amounts = JsonObjectSet(amounts, "custom7", JsonInt(d.iCustom7)); total += d.iCustom7; }
    if (d.iCustom8 > 0) { amounts = JsonObjectSet(amounts, "custom8", JsonInt(d.iCustom8)); total += d.iCustom8; }
    if (d.iCustom9 > 0) { amounts = JsonObjectSet(amounts, "custom9", JsonInt(d.iCustom9)); total += d.iCustom9; }
    if (d.iCustom10 > 0) { amounts = JsonObjectSet(amounts, "custom10", JsonInt(d.iCustom10)); total += d.iCustom10; }
    if (d.iCustom11 > 0) { amounts = JsonObjectSet(amounts, "custom11", JsonInt(d.iCustom11)); total += d.iCustom11; }
    if (d.iCustom12 > 0) { amounts = JsonObjectSet(amounts, "custom12", JsonInt(d.iCustom12)); total += d.iCustom12; }
    if (d.iCustom13 > 0) { amounts = JsonObjectSet(amounts, "custom13", JsonInt(d.iCustom13)); total += d.iCustom13; }
    if (d.iCustom14 > 0) { amounts = JsonObjectSet(amounts, "custom14", JsonInt(d.iCustom14)); total += d.iCustom14; }
    if (d.iCustom15 > 0) { amounts = JsonObjectSet(amounts, "custom15", JsonInt(d.iCustom15)); total += d.iCustom15; }
    if (d.iCustom16 > 0) { amounts = JsonObjectSet(amounts, "custom16", JsonInt(d.iCustom16)); total += d.iCustom16; }
    if (d.iCustom17 > 0) { amounts = JsonObjectSet(amounts, "custom17", JsonInt(d.iCustom17)); total += d.iCustom17; }
    if (d.iCustom18 > 0) { amounts = JsonObjectSet(amounts, "custom18", JsonInt(d.iCustom18)); total += d.iCustom18; }
    if (d.iCustom19 > 0) { amounts = JsonObjectSet(amounts, "custom19", JsonInt(d.iCustom19)); total += d.iCustom19; }
    e = JsonObjectSet(e, "damage", amounts);
    e = JsonObjectSet(e, "amount", JsonInt(total));
    e = JsonObjectSet(e, "spell", JsonInt(d.iSpellId));
    MgLiveQueueEvent(e);
    SetLocalObject(OBJECT_SELF, "MGLIVE_DAMAGER", d.oDamager);
    // Damage callback is BEFORE application. Observe the actual resulting state.
    DelayCommand(0.01, MgLiveObserveDeath(OBJECT_SELF,JsonGetString(JsonObjectGet(e,"command_id")),JsonGetString(JsonObjectGet(e,"by"))));
}
