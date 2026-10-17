void main()
{
    object other = GetLocalObject(GetModule(), "MGET_CALM_OTHER");
    // This binary's getter evaluates its SECOND object's reputation towards first.
    SetName(OBJECT_SELF, "rep=" + IntToString(GetReputation(other, OBJECT_SELF)) +
        " enemy=" + IntToString(GetIsEnemy(other, OBJECT_SELF)) +
        " cmd=" + IntToString(GetCommandable(OBJECT_SELF)));
}
