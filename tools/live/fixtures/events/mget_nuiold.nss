void main()
{
    object m = GetModule();
    SetLocalInt(m, "MGET_NUI_FORWARDED", GetLocalInt(m, "MGET_NUI_FORWARDED") + 1);
}
