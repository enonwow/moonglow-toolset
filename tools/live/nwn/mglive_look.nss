#include "mglive_inc"

// Scheduled by the report preflight; one appearance build per fresh VM budget.
void main()
{
    if (!GetIsObjectValid(OBJECT_SELF) || GetObjectType(OBJECT_SELF) != OBJECT_TYPE_CREATURE) return;
    DeleteLocalString(OBJECT_SELF, "MGLIVE_LOOK_SIGNATURE");
    MgLiveNpcLook(OBJECT_SELF);
}
