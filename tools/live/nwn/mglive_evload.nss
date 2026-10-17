#include "nwnx_chat"
#include "nwnx_damage"
#include "nwnx_events"

void main()
{
    NWNX_Chat_RegisterChatScript("mglive_chat");
    NWNX_Damage_SetAttackEventScript("mglive_attack");
    NWNX_Damage_SetDamageEventScript("mglive_damage");
    NWNX_Events_SubscribeEvent(NWNX_ON_CAST_SPELL_AFTER, "mglive_spell");
    NWNX_Events_SubscribeEvent(NWNX_ON_BROADCAST_SAFE_PROJECTILE_AFTER, "mglive_vfx");
    NWNX_Events_SubscribeEvent(NWNX_ON_CLIENT_CONNECT_AFTER, "mglive_player");
    NWNX_Events_SubscribeEvent(NWNX_ON_CLIENT_DISCONNECT_BEFORE, "mglive_player");
    NWNX_Events_SubscribeEvent(NWNX_ON_CLIENT_DISCONNECT_AFTER, "mglive_player");
}
