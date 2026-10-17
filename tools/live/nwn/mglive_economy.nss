// Caller has already resolved area, player protection and living-target gates.
json MgLiveEconomy(json j, object target)
{
    string id=JsonGetString(JsonObjectGet(j,"id"));
    string op=JsonGetString(JsonObjectGet(j,"op"));
    if(GetObjectType(target)!=OBJECT_TYPE_CREATURE) return MgLiveResult(id,"target must be a creature");
    int gold=op=="take_gold";
    int before=GetXP(target); if(gold) before=GetGold(target);
    int level=GetHitDice(target); int after;
    if(op=="remove_level")
    {
        if(level<=1 || level>40) return MgLiveResult(id,"supported current levels are 2-40");
        string threshold=Get2DAString("exptable","XP",level-2);
        if(threshold=="" || threshold=="****") return MgLiveResult(id,"previous level threshold unavailable",OBJECT_INVALID,"not_found");
        after=StringToInt(threshold);
        if(after<0 || after>=before) return MgLiveResult(id,"previous threshold must reduce current XP");
    }
    else if(op=="set_xp")
    {
        json value=JsonObjectGet(j,"value");
        if(!MgLiveInt(value,0,2147483647)) return MgLiveResult(id,"value must be integer 0-2147483647");
        after=JsonGetInt(value);
    }
    else
    {
        json amount=JsonObjectGet(j,"amount"); int limit=100000; if(gold) limit=1000000;
        if(!MgLiveInt(amount,1,limit)) return MgLiveResult(id,"amount outside supported positive integer range");
        int remove=JsonGetInt(amount); if(remove>before) remove=before;
        after=before-remove;
    }
    if(gold)
    {
        SetLocalInt(target,"MGLIVE_TAKE_GOLD",before-after);
        ExecuteScript("mglive_gold",target);
    }
    else SetXP(target,after);
    int actual=GetXP(target); if(gold) actual=GetGold(target);
    json value=JsonObjectSet(JsonObject(),"before",JsonInt(before));
    value=JsonObjectSet(value,"after",JsonInt(actual));
    value=JsonObjectSet(value,"level_before",JsonInt(level));
    value=JsonObjectSet(value,"level_after",JsonInt(GetHitDice(target)));
    return JsonObjectSet(MgLiveResult(id),"value",value);
}
