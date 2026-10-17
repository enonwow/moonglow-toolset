// Preserve game bytes before native JsonString can apply the wrong charset.
// Only non-ASCII values need a companion. Rust decodes these in wire_text.rs.
#include "nwnx_util"
string MgLiveTextEscape(string s, string needle, string replacement)
{
    int at = FindSubString(s, needle);
    string out = "";
    while (at >= 0)
    {
        out += GetStringLeft(s, at) + replacement;
        s = GetSubString(s, at + 1, GetStringLength(s));
        at = FindSubString(s, needle);
    }
    return out + s;
}

json MgLiveTextField(json j, string key, string s)
{
    j = JsonObjectSet(j, key, JsonString(s));
    // NWNX's URL helper leaves literal % and + untouched. Escape those first.
    string bytes = NWNX_Util_EncodeStringForURL(MgLiveTextEscape(MgLiveTextEscape(s, "%", "%25"), "+", "%2B"));
    if (FindSubString(bytes, "%8") >= 0 || FindSubString(bytes, "%9") >= 0 ||
        FindSubString(bytes, "%A") >= 0 || FindSubString(bytes, "%B") >= 0 ||
        FindSubString(bytes, "%C") >= 0 || FindSubString(bytes, "%D") >= 0 ||
        FindSubString(bytes, "%E") >= 0 || FindSubString(bytes, "%F") >= 0)
        j = JsonObjectSet(j, key + "_bytes", JsonString(bytes));
    return j;
}

string MgLiveTextCodepage()
{
    string cp = NWNX_Util_GetEnvironmentVariable("MGLIVE_TEXT_CODEPAGE");
    if (cp != "cp1251" && cp != "cp1252") cp = "cp1250";
    return cp;
}
