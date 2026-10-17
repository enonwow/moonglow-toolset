// NWN1 has no PlayCustomAnimation. Preserve a native replacement while borrowing
// one slot, and restore only if nobody else replaced it since our command.
string MgLiveAnimationMapping(object actor, string slot)
{
    json data=JsonObjectGet(JsonObjectGet(ObjectToJson(actor),"AnimationReplace"),"value");
    json rows=JsonObjectGet(JsonObjectGet(data,"AnimationReplace"),"value"); int i;
    for(i=0;i<JsonGetLength(rows);i++)
    {
        json row=JsonArrayGet(rows,i);
        if(JsonGetString(JsonObjectGet(JsonObjectGet(row,"OldAnimation"),"value"))==slot)
            return JsonGetString(JsonObjectGet(JsonObjectGet(row,"NewAnimation"),"value"));
    }
    return "";
}

void MgLiveCancelAnimation(object actor)
{
    if(!GetLocalInt(actor,"MGLIVE_ANIM_ACTIVE")) return;
    DeleteLocalInt(actor,"MGLIVE_ANIM_ACTIVE");
    DeleteLocalInt(actor,"MGLIVE_ANIM_CYCLES");
    string slot=GetLocalString(actor,"MGLIVE_ANIM_SLOT");
    if(slot!="" && MgLiveAnimationMapping(actor,slot)==GetLocalString(actor,"MGLIVE_ANIM_NEW"))
        ReplaceObjectAnimation(actor,slot,GetLocalString(actor,"MGLIVE_ANIM_PREVIOUS"));
    DeleteLocalString(actor,"MGLIVE_ANIM_SLOT"); DeleteLocalString(actor,"MGLIVE_ANIM_NEW");
    DeleteLocalString(actor,"MGLIVE_ANIM_PREVIOUS"); DeleteLocalJson(actor,"MGLIVE_ANIMATION");
    SetCommandable(TRUE,actor);
    AssignCommand(actor,ClearAllActions(TRUE));
}

void MgLiveAnimationEnd(object actor, int generation)
{
    if(GetIsObjectValid(actor) && GetLocalInt(actor,"MGLIVE_ANIM_ACTIVE")==generation)
        MgLiveCancelAnimation(actor);
}

void MgLiveAnimationCycle(object actor,int generation,int animation,float speed,float seconds,int repeat)
{
    if(!GetIsObjectValid(actor) || GetIsDead(actor) || GetLocalInt(actor,"MGLIVE_ANIM_ACTIVE")!=generation) return;
    int cycles=GetLocalInt(actor,"MGLIVE_ANIM_CYCLES")+1;
    SetLocalInt(actor,"MGLIVE_ANIM_CYCLES",cycles);
    SetLocalJson(actor,"MGLIVE_ANIMATION",JsonObjectSet(GetLocalJson(actor,"MGLIVE_ANIMATION"),"cycles",JsonInt(cycles)));
    SetCommandable(TRUE,actor);
    ActionPlayAnimation(animation,speed,seconds);
    if(repeat) ActionDoCommand(MgLiveAnimationCycle(actor,generation,animation,speed,seconds,repeat));
    SetCommandable(FALSE,actor);
}

json MgLiveCreatureAnimation(json j,object actor)
{
    string id=JsonGetString(JsonObjectGet(j,"id"));
    if(GetObjectType(actor)!=OBJECT_TYPE_CREATURE) return MgLiveResult(id,"target must be a creature");
    json animation=JsonObjectGet(j,"animation"); json loop=JsonObjectGet(j,"loop");
    json speed=JsonObjectGet(j,"speed"); json seconds=JsonObjectGet(j,"seconds");
    if(JsonGetType(loop)==JSON_TYPE_NULL) loop=JsonBool(FALSE);
    if(JsonGetType(speed)==JSON_TYPE_NULL) speed=JsonFloat(1.0);
    if(JsonGetType(seconds)==JSON_TYPE_NULL) seconds=JsonFloat(30.0);
    if(JsonGetType(loop)!=JSON_TYPE_BOOL || !MgLiveNumber(speed) || !MgLiveNumber(seconds) ||
       JsonGetFloat(speed)<0.1 || JsonGetFloat(speed)>10.0 || JsonGetFloat(seconds)<0.1 || JsonGetFloat(seconds)>300.0)
        return MgLiveResult(id,"loop boolean, speed 0.1-10 and seconds 0.1-300 required");
    int n; string slot=""; string name="";
    if(JsonGetType(animation)==JSON_TYPE_INTEGER)
    {
        if(!MgLiveInt(animation,0,92) && !MgLiveInt(animation,100,116)) return MgLiveResult(id,"invalid creature ANIMATION constant");
        n=JsonGetInt(animation);
    }
    else if(JsonGetType(animation)==JSON_TYPE_STRING && MgLiveResRef(JsonGetString(animation)))
    {
        name=JsonGetString(animation);
        if(JsonGetInt(loop)) {slot="custom70"; n=ANIMATION_LOOPING_CUSTOM70;}
        else {slot="salute"; n=ANIMATION_FIREFORGET_SALUTE;}
    }
    else return MgLiveResult(id,"animation must be a creature constant or model animation resref");
    int generation=MgLiveNewOrder(actor);
    if(slot!="")
    {
        SetLocalString(actor,"MGLIVE_ANIM_PREVIOUS",MgLiveAnimationMapping(actor,slot));
        SetLocalString(actor,"MGLIVE_ANIM_SLOT",slot); SetLocalString(actor,"MGLIVE_ANIM_NEW",name);
        ReplaceObjectAnimation(actor,slot,name);
    }
    SetLocalInt(actor,"MGLIVE_ANIM_ACTIVE",generation);
    SetLocalInt(actor,"MGLIVE_ANIM_CYCLES",0);
    json state=JsonObjectSet(JsonObject(),"animation",animation);
    state=JsonObjectSet(state,"loop",loop);state=JsonObjectSet(state,"speed",speed);state=JsonObjectSet(state,"seconds",seconds);
    state=JsonObjectSet(state,"native_constant",JsonInt(n));
    if(slot!="")state=JsonObjectSet(state,"replacement_slot",JsonString(slot));
    SetLocalJson(actor,"MGLIVE_ANIMATION",state);
    AssignCommand(actor,ClearAllActions(TRUE));
    AssignCommand(actor,MgLiveAnimationCycle(actor,generation,n,JsonGetFloat(speed),JsonGetFloat(seconds),JsonGetInt(loop) && n>=100));
    DelayCommand(JsonGetFloat(seconds),MgLiveAnimationEnd(actor,generation));
    return JsonObjectSet(MgLiveResult(id),"value",state);
}
