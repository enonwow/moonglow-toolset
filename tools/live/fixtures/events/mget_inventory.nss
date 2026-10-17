void main()
{
    int count;
    object item = GetFirstItemInInventory();
    string found;
    while (GetIsObjectValid(item))
    {
        if (GetResRef(item) == "nw_it_mpotion001")
        {
            count += GetItemStackSize(item);
            found = ObjectToString(item);
        }
        item = GetNextItemInInventory();
    }
    SetName(OBJECT_SELF, "potions=" + IntToString(count) + " item=" + found);
}
