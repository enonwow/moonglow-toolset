// The native equipment mask is authoritative (module baseitems overrides included).
int MgLiveEquipSlots(object item)
{
    string raw=GetStringLowerCase(Get2DAString("baseitems","EquipableSlots",GetBaseItemType(item)));
    int base=10;int i=0;int value=0;int digit;
    if(GetStringLeft(raw,2)=="0x") { base=16;i=2; }
    for(;i<GetStringLength(raw);i++)
    {
        digit=FindSubString("0123456789abcdef",GetSubString(raw,i,1));
        if(digit<0 || digit>=base || value>262143) return 0;
        value=value*base+digit;
    }
    return value & 262143;
}

// Preserve property indices: an item may have several cast abilities.
json MgLiveItemUses(object item)
{
    json uses=JsonArray();int index;itemproperty ip=GetFirstItemProperty(item);
    while(GetIsItemPropertyValid(ip) && index<256)
    {
        if(GetItemPropertyType(ip)==ITEM_PROPERTY_CAST_SPELL)
        {
            int row=GetItemPropertySubType(ip);
            int spell=StringToInt(Get2DAString("iprp_spells","SpellIndex",row));
            string label=GetStringByStrRef(StringToInt(Get2DAString("iprp_spells","Name",row)));
            json use=JsonObjectSet(JsonObject(),"property",JsonInt(index));
            use=JsonObjectSet(use,"spell",JsonInt(spell));
            use=MgLiveTextField(use,"name",label);
            uses=JsonArrayInsert(uses,use);
        }
        index++;ip=GetNextItemProperty(item);
    }
    return uses;
}

// Numeric icon inputs from the actual item, not a possibly different blueprint.
json MgLiveItemLook(object item)
{
    json source=ObjectToJson(item); json keys=JsonObjectKeys(source); json look=JsonObject(); int i;
    for(i=0;i<JsonGetLength(keys);i++)
    {
        string key=JsonGetString(JsonArrayGet(keys,i));
        if(key=="BaseItem" || FindSubString(key,"ModelPart")==0 || FindSubString(key,"xModelPart")==0 ||
           FindSubString(key,"ArmorPart")==0 || FindSubString(key,"xArmorPart")==0 ||
           FindSubString(key,"Color")>=0)
        {
            json v=JsonObjectGet(JsonObjectGet(source,key),"value");
            if(JsonGetType(v)==JSON_TYPE_INTEGER) look=JsonObjectSet(look,key,v);
        }
    }
    return look;
}

json MgLiveInventoryOrder(json j,object actor,int allowPlayers)
{
    string id=JsonGetString(JsonObjectGet(j,"id"));string op=JsonGetString(JsonObjectGet(j,"op"));
    if(GetObjectType(actor)!=OBJECT_TYPE_CREATURE) return MgLiveResult(id,"target must be a creature");
    string itemId=JsonGetString(JsonObjectGet(j,"item"));
    if(!MgLiveObjectId(itemId)) return MgLiveResult(id,"invalid item id");
    object item=MgLiveFindObject(itemId);
    if(!GetIsObjectValid(item) || GetObjectType(item)!=OBJECT_TYPE_ITEM || GetItemPossessor(item)!=actor)
        return MgLiveResult(id,"item is not in the selected inventory",OBJECT_INVALID,"not_found");
    if(op=="use_item" || op=="transfer_item")
    {
        string recipientId=JsonGetString(JsonObjectGet(j,"recipient"));
        if(op=="use_item" && JsonGetType(JsonObjectGet(j,"recipient"))==JSON_TYPE_NULL) recipientId=ObjectToString(actor);
        if(!MgLiveObjectId(recipientId)) return MgLiveResult(id,"invalid recipient id");
        object recipient=MgLiveFindObject(recipientId);
        if(!GetIsObjectValid(recipient)) return MgLiveResult(id,"recipient not found",OBJECT_INVALID,"not_found");
        if(GetArea(recipient)!=GetArea(actor)) return MgLiveResult(id,"recipient in another area",OBJECT_INVALID,"other_area");
        if(GetObjectType(recipient)!=OBJECT_TYPE_CREATURE) return MgLiveResult(id,"recipient must be a creature");
        if(MgLiveProtected(recipient) && !allowPlayers) return MgLiveResult(id,"player recipient requires both switches",OBJECT_INVALID,"disabled");
        if(GetIsDead(recipient)) return MgLiveResult(id,"recipient is dead",OBJECT_INVALID,"dead");
        if(op=="transfer_item")
        {
            if(recipient==actor) return MgLiveResult(id,"recipient already holds item");
            if(GetPlotFlag(item) || !GetDroppableFlag(item)) return MgLiveResult(id,"item is plot or not droppable",OBJECT_INVALID,"protected");
            object copy=CopyObject(item,GetLocation(recipient),recipient,"",TRUE);
            if(!GetIsObjectValid(copy) || GetItemPossessor(copy)!=recipient)
            {
                if(GetIsObjectValid(copy)) DestroyObject(copy);
                return MgLiveResult(id,"native inventory copy failed; original retained");
            }
            DestroyObject(item);
            json result=JsonObjectSet(JsonObject(),"previous",JsonString(itemId));
            result=JsonObjectSet(result,"item",JsonString(ObjectToString(copy)));
            result=JsonObjectSet(result,"recipient",JsonString(recipientId));
            return JsonObjectSet(MgLiveResult(id,"",copy),"value",result);
        }
        json property=JsonObjectGet(j,"property");int requested=0;
        if(JsonGetType(property)!=JSON_TYPE_NULL)
        { if(!MgLiveInt(property,0,255)) return MgLiveResult(id,"property must be 0-255"); requested=JsonGetInt(property); }
        itemproperty ip=GetFirstItemProperty(item);int index;
        while(GetIsItemPropertyValid(ip) && index<requested) { ip=GetNextItemProperty(item);index++; }
        if(!GetIsItemPropertyValid(ip) || GetItemPropertyType(ip)!=ITEM_PROPERTY_CAST_SPELL)
            return MgLiveResult(id,"property is not a cast-spell ability");
        MgLiveCancelHold(actor);
        if(!GetCommandable(actor)) return MgLiveResult(id,"actor is not commandable",OBJECT_INVALID,"disabled");
        AssignCommand(actor,ClearAllActions(TRUE));
        // Engine checks use restrictions and consumes charges/stack, as a client action does.
        AssignCommand(actor,ActionUseItemOnObject(item,ip,recipient,0,TRUE));
        return JsonObjectSet(MgLiveResult(id),"value",JsonObjectSet(JsonObject(),"queued",JsonBool(TRUE)));
    }
    json slot=JsonObjectGet(j,"slot");int n=-1;int ok;
    if(op=="drop_item")
    {
        if(GetPlotFlag(item)) return MgLiveResult(id,"plot items cannot be dropped",OBJECT_INVALID,"protected");
        int forced=!GetDroppableFlag(item);
        // A DM drop must not wait on an actor's AI/action queue. The public
        // native API has no immediate transfer-to-ground primitive; recreate
        // the exact runtime item, including locals, before removing the held one.
        // Identity changes are explicit in the reply. Never delete on copy failure.
        object dropped=CopyObject(item,GetLocation(actor),OBJECT_INVALID,"",TRUE);
        if(!GetIsObjectValid(dropped) || GetItemPossessor(dropped)!=OBJECT_INVALID || GetArea(dropped)!=GetArea(actor))
        {
            if(GetIsObjectValid(dropped)) DestroyObject(dropped);
            return MgLiveResult(id,"native ground copy failed; original item retained");
        }
        SetDroppableFlag(dropped,!forced);
        DestroyObject(item);
        json value=JsonObjectSet(JsonObject(),"previous",JsonString(itemId));
        value=JsonObjectSet(value,"item",JsonString(ObjectToString(dropped)));
        value=JsonObjectSet(value,"queued",JsonBool(FALSE));
        if(forced) value=JsonObjectSet(value,"droppable_overridden",JsonBool(TRUE));
        return JsonObjectSet(MgLiveResult(id,"",dropped),"value",value);
    }
    if(!MgLiveInt(slot,0,13)) return MgLiveResult(id,"slot must be integer 0-13");
    n=JsonGetInt(slot);
    if(op=="equip_item" && !(MgLiveEquipSlots(item) & (1 << n)))
        return MgLiveResult(id,"baseitems.2da does not allow this item in that slot");
    if(op=="unequip_item" && GetItemInSlot(n,actor)!=item) return MgLiveResult(id,"item is not in that equipped slot");
    MgLiveCancelHold(actor);
    if(op=="equip_item") ok=NWNX_Creature_RunEquip(actor,item,n);
    else ok=NWNX_Creature_RunUnequip(actor,item);
    int actual=-1;int i;
    for(i=0;i<NUM_INVENTORY_SLOTS;i++) if(GetItemInSlot(i,actor)==item) actual=i;
    if(GetItemPossessor(item)!=actor || (op=="equip_item" && actual!=n) || (op=="unequip_item" && actual>=0))
        return MgLiveResult(id,"native equip/unequip refused: returned " + IntToString(ok) + ", slot " + IntToString(actual));
    json value=JsonObjectSet(JsonObject(),"item",JsonString(itemId));
    value=JsonObjectSet(value,"slot",JsonInt(actual));
    value=JsonObjectSet(value,"native_return",JsonInt(ok));
    return JsonObjectSet(MgLiveResult(id),"value",value);
}
