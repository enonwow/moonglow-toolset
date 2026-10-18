// Compiled VM context: NWNX calls must not run through ExecuteScriptChunk.
#include "mglive_xl"
void main() {
    object m = GetModule();
    object speaker = GetLocalObject(m, "MGET_XL_SPEAKER");
    int channel = GetLocalInt(m, "MGET_XL_CHANNEL");
    string label = GetLocalString(m, "MGET_XL_LABEL");
    object target = GetObjectByTag("XL_NEAR");
    string text = label + " hello";
    int limit = NWNX_Util_GetInstructionLimit();
    int start = NWNX_Util_GetInstructionsExecuted();
    int captured = MgXlCapture(channel, text, speaker, target);
    int used = NWNX_Util_GetInstructionsExecuted() - start;
    json row = JsonObjectSet(JsonObject(), "instructions", JsonInt(used));
    JsonObjectSetInplace(row, "captured", JsonInt(captured));
    JsonObjectSetInplace(row, "limit", JsonInt(limit));
    JsonObjectSetInplace(row, "channel", JsonInt(channel));
    JsonObjectSetInplace(row, "label", JsonString(label));
    SetLocalJson(m, "MGET_XL_BUDGET", row);
}
