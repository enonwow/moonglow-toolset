// Module-owned TLK dialogue entry, not text supplied over Redis.
void main()
{
    string line = GetStringByStrRef(16777216);
    // Deliberately UTF-8 source literal, as found in the owner's NPC script.
    if (GetLocalInt(GetModule(), "MGET_POLISH_UTF8")) line = "Naprawdę: ąęłńóśżź + %C4%85";
    SetName(OBJECT_SELF, line);
    SetLocalInt(GetModule(), "MGET_POLISH_BYTES", GetStringLength(line));
    SpeakString(line);
    WriteTimestampedLogEntry("MGET_POLISH " + line);
}
