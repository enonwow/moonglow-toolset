#include "mglive_nui"
void main()
{
    SetLocalJson(GetModule(), "MGET_NUI_EMPTY", MgLiveSkillWindow(1, 15, 2, FALSE));
    object m = GetModule();
    SetEventScript(m, EVENT_SCRIPT_MODULE_ON_NUI_EVENT, "mget_nuiold");
    MgLiveNuiInstall();
    MgLiveNuiInstall(); // Re-entry must not replace the saved module handler.
    ExecuteScript("mglive_nuievt", m); // No current window: forwards to prior handler.
    json proof = JsonObject();
    proof = JsonObjectSet(proof, "window", MgLiveSkillWindow(SKILL_CONCENTRATION, 15, 2, TRUE));
    proof = JsonObjectSet(proof, "failure", MgLiveSkillWindow(SKILL_CONCENTRATION, 25, -2, FALSE));
    proof = JsonObjectSet(proof, "zero_bonus", MgLiveSkillWindow(SKILL_CONCENTRATION, 15, 0, FALSE));
    proof = JsonObjectSet(proof, "handler", JsonString(GetEventScript(m, EVENT_SCRIPT_MODULE_ON_NUI_EVENT)));
    proof = JsonObjectSet(proof, "previous", JsonString(GetLocalString(m, "MGLIVE_NUI_PREVIOUS")));
    proof = JsonObjectSet(proof, "forwarded", JsonInt(GetLocalInt(m, "MGET_NUI_FORWARDED")));
    // Real native call, deliberately no PC: zero is expected, not a render proof.
    proof = JsonObjectSet(proof, "no_client_token", JsonInt(NuiCreate(OBJECT_INVALID, MgLiveSkillWindow(SKILL_CONCENTRATION, 15, 2, TRUE), MGLIVE_SKILL_WINDOW)));
    SetLocalJson(m, "MGET_NUI_PROOF", proof);
}
