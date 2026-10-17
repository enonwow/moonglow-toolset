void main()
{
    PlaySound(GetLocalString(OBJECT_SELF,"MGLIVE_SOUND"));
    SetLocalInt(OBJECT_SELF,"MGLIVE_SOUND_SUBMITTED",TRUE);
}
