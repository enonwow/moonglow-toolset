// Observe native commandability without issuing a command that cancels a hold.
void ProbeControl(int remaining)
{
    SetName(OBJECT_SELF, "commandable=" + IntToString(GetCommandable(OBJECT_SELF)));
    if (remaining > 0) DelayCommand(0.1, ProbeControl(remaining - 1));
}
void main() { ProbeControl(300); }
