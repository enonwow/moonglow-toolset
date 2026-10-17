// Native TakeGoldFromCreature requires a creature caller, even with bDestroy.
// The validated dispatcher executes this synchronously on the target.
void main()
{
    int amount=GetLocalInt(OBJECT_SELF,"MGLIVE_TAKE_GOLD");
    DeleteLocalInt(OBJECT_SELF,"MGLIVE_TAKE_GOLD");
    if(amount>0 && amount<=1000000) TakeGoldFromCreature(amount,OBJECT_SELF,TRUE);
}
