// A DM reply without a connected DM avatar: targeted native server feedback.
// The normal dispatcher enforces both player permissions and a real PC target.
json MgLiveDmReply(object target, string text)
{
    string line = "DM: " + text;
    SendMessageToPC(target, line);
    json result = JsonObjectSet(JsonObject(), "native", JsonString("SendMessageToPC"));
    result = JsonObjectSet(result, "recipient", JsonString(ObjectToString(target)));
    result = JsonObjectSet(result, "submitted", JsonBool(TRUE));
    result = JsonObjectSet(result, "delivery_verified", JsonBool(FALSE));
    return MgLiveTextField(result, "text", line);
}
