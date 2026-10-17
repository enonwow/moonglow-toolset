// Deliberate private negative control: a later DelayCommand must get a fresh VM budget.
void main()
{
    SetLocalString(GetModule(),"MP_VM_PHASE","after_tmi");
    DelayCommand(0.1,ExecuteScript("mp_vmchild",GetModule()));
    int i=0; while(TRUE) i++;
}
