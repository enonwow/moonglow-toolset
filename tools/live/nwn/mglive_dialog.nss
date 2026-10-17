// Opt-in collector for a dialogue node's Actions Taken script. Include this
// there and call MgLiveDialogLine("actual_dlg_resref") while native context lives.
// Never call from a conditional: offered replies are not chosen replies.
#include "mglive_evt"
#include "nwnx_dialog"

int MgLiveDialogLine(string dialog, object speaker = OBJECT_INVALID,
    object listener = OBJECT_INVALID, int language = NWNX_DIALOG_LANGUAGE_POLISH)
{
    if (NWNX_Dialog_GetCurrentScriptType() != NWNX_DIALOG_SCRIPT_TYPE_ACTION_TAKEN) return FALSE;
    int type = NWNX_Dialog_GetCurrentNodeType();
    if (type != NWNX_DIALOG_NODE_TYPE_ENTRY_NODE && type != NWNX_DIALOG_NODE_TYPE_REPLY_NODE) return FALSE;
    int node = NWNX_Dialog_GetCurrentNodeID();
    if (node < 0 || dialog == "" || GetStringLength(dialog) > 16) return FALSE;
    int reply = type == NWNX_DIALOG_NODE_TYPE_REPLY_NODE;
    // Explicit overrides support DLG nodes with a different tagged speaker.
    // OBJECT_SELF is the conversation owner, not necessarily that tagged actor.
    if (!GetIsObjectValid(speaker)) speaker = reply ? GetPCSpeaker() : OBJECT_SELF;
    if (!GetIsObjectValid(speaker) && reply) speaker = GetLastSpeaker();
    if (!GetIsObjectValid(listener))
    {
        listener = reply ? OBJECT_SELF : GetPCSpeaker();
        if (!GetIsObjectValid(listener) && !reply) listener = GetLastSpeaker();
    }
    json e = MgLiveEvent("dialog", speaker, listener);
    e = JsonObjectSet(e, "speaker", JsonObjectGet(e, "id"));
    e = JsonObjectSet(e, "listener", JsonObjectGet(e, "target"));
    e = JsonObjectSet(e, "owner", JsonString(ObjectToString(OBJECT_SELF)));
    e = JsonObjectSet(e, "speaker_is_pc", GetIsObjectValid(speaker) ? JsonBool(GetIsPC(speaker)) : JsonNull());
    if (!GetIsObjectValid(speaker) && GetIsObjectValid(GetArea(OBJECT_SELF)))
        e = JsonObjectSet(e, "area", JsonString(GetResRef(GetArea(OBJECT_SELF))));
    e = JsonObjectSet(e, "dialog", JsonString(dialog));
    e = JsonObjectSet(e, "node", JsonInt(node));
    e = JsonObjectSet(e, "node_type", JsonString(reply ? "reply" : "entry"));
    e = JsonObjectSet(e, "language", JsonInt(language));
    // NWNX returns the locstring, before client token substitution. Do not
    // manufacture a chat channel or imply the final text reached a client.
    e = JsonObjectSet(e, "text_source", JsonString("node_locstring"));
    int gender = GENDER_MALE;
    if (GetIsObjectValid(speaker) && GetObjectType(speaker) == OBJECT_TYPE_CREATURE)
        gender = GetGender(speaker) == GENDER_FEMALE ? GENDER_FEMALE : GENDER_MALE;
    e = MgLiveTextField(e, "text", NWNX_Dialog_GetCurrentNodeText(language, gender));
    MgLiveQueueEvent(e);
    return TRUE;
}
