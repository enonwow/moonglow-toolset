// mg-live.v1 helpers. Only authored scripts belong in the repository.
#include "nwnx_util"
#include "nwnx_creature"
#include "nwnx_object"
#include "mglive_evt"
#include "mglive_receipt"
const float MGLIVE_PERIOD = 1.0;
const int MGLIVE_FULL_EVERY = 5;
const int MGLIVE_DELTA_FULL_EVERY = 30;
const int MGLIVE_DELTA_THRESHOLD = 100000;
const int MGLIVE_MAX_DONE = 256;

// First observed appearance in this module run, scoped to this object ID so a
// library/copy with inherited locals starts its own baseline.
int MgLiveOriginalAppearance(object o, int appearance)
{
    string identity=GetLocalString(GetModule(),"MGLIVE_BOOT")+":"+ObjectToString(o);
    if(GetLocalString(o,"MGLIVE_APP_AT")!=identity)
    {
        SetLocalString(o,"MGLIVE_APP_AT",identity);
        SetLocalInt(o,"MGLIVE_APP_ORIGINAL",appearance);
    }
    return GetLocalInt(o,"MGLIVE_APP_ORIGINAL");
}

// Read once at module load, before script/resource overrides can change the
// template. Runtime faction creation is not supported; IDs are FAC list order.
json MgLiveServerFactions()
{
    json rows = JsonObjectGet(JsonObjectGet(TemplateToJson("repute", RESTYPE_FAC), "FactionList"), "value");
    if (JsonGetType(rows) != JSON_TYPE_ARRAY || JsonGetLength(rows) > 65536) return JsonNull();
    json result = JsonArray();
    int i;
    for (i = 0; i < JsonGetLength(rows); i++)
    {
        string name = JsonGetString(JsonObjectGet(JsonObjectGet(JsonArrayGet(rows, i), "FactionName"), "value"));
        json entry = JsonObjectSet(JsonObject(), "id", JsonInt(i));
        JsonArrayInsertInplace(result, MgLiveTextField(entry, "name", name));
    }
    return result;
}

void MgLiveConfigure()
{
    object m = GetModule();
    string prefix = NWNX_Util_GetEnvironmentVariable("MGLIVE_PREFIX");
    if (prefix == "") prefix = "mglive";
    SetLocalString(m, "MGLIVE_PREFIX", prefix);
    SetLocalString(m, "MGLIVE_AREAS", NWNX_Util_GetEnvironmentVariable("MGLIVE_AREAS"));
    SetLocalString(m, "MGLIVE_ALLOW_CHUNK", NWNX_Util_GetEnvironmentVariable("MGLIVE_ALLOW_CHUNK"));
    SetLocalString(m, "MGLIVE_ALLOW_PLAYER_OPS", NWNX_Util_GetEnvironmentVariable("MGLIVE_ALLOW_PLAYER_OPS"));
    // The launcher supplies cryptographic randomness across server restarts. A module
    // reload in the same server (the last player leaving) starts the report and event
    // sequences at 1 again, so each load is a run of its own.
    string boot = NWNX_Util_GetEnvironmentVariable("MGLIVE_BOOT");
    if (boot == "") boot = IntToString(Random(1000000000));
    boot += "-" + IntToString(GetMicrosecondCounter());
    SetLocalString(m, "MGLIVE_BOOT", boot);
    SetLocalJson(m, "MGLIVE_FACTIONS", MgLiveServerFactions());
}

int MgLiveContains(json a, string id)
{
    // Search in native code: 256 acknowledgements must not exhaust VM instructions.
    return JsonGetType(JsonFind(a, JsonString(id))) != JSON_TYPE_NULL;
}

int MgLiveResRef(string s)
{
    int n = GetStringLength(s);
    if (n < 1 || n > 16) return FALSE;
    string chars = "abcdefghijklmnopqrstuvwxyz0123456789_-";
    s = GetStringLowerCase(s);
    int i;
    for (i = 0; i < n; i++)
        if (FindSubString(chars, GetSubString(s, i, 1)) < 0) return FALSE;
    return TRUE;
}

object MgLiveFindArea(string s)
{
    if (!MgLiveResRef(s)) return OBJECT_INVALID;
    s = GetStringLowerCase(s);
    object a = GetFirstArea();
    while (GetIsObjectValid(a))
    {
        if (GetStringLowerCase(GetResRef(a)) == s) return a;
        a = GetNextArea();
    }
    return OBJECT_INVALID;
}

int MgLiveObjectId(string s)
{
    int n = GetStringLength(s);
    if (n < 1 || n > 16) return FALSE;
    int i;
    for (i = 0; i < n; i++)
        if (FindSubString("0123456789abcdef", GetStringLowerCase(GetSubString(s, i, 1))) < 0)
            return FALSE;
    return TRUE;
}

object MgLiveFindObject(string s)
{
    if (!MgLiveObjectId(s)) return OBJECT_INVALID;
    // Native EE counterpart of ObjectToString; no Object plugin wrapper exists.
    return StringToObject(s);
}

int MgLiveNumber(json j)
{
    int t = JsonGetType(j);
    if (t != JSON_TYPE_INTEGER && t != JSON_TYPE_FLOAT) return FALSE;
    float f = JsonGetFloat(j);
    return f == f && (f - f) == 0.0; // Refuse NaN and infinities after float conversion.
}

int MgLivePosition(json j)
{
    return MgLiveNumber(JsonObjectGet(j, "x")) &&
           MgLiveNumber(JsonObjectGet(j, "y")) &&
           MgLiveNumber(JsonObjectGet(j, "z"));
}

int MgLiveDirectPlayer(object o)
{
    return GetIsPC(o) || GetIsDM(o) || GetIsDMPossessed(o);
}

int MgLiveProtected(object o)
{
    if (MgLiveDirectPlayer(o)) return TRUE;
    object master = GetMaster(o);
    return GetIsObjectValid(master) && MgLiveDirectPlayer(master);
}

int MgLiveText(string s)
{
    int n = GetStringLength(s);
    if (n < 1 || n > 200) return FALSE;
    // NWScript's source escape set does not reliably preserve \t/\r. Obtain
    // actual control bytes from JSON, as incoming command text does.
    string whitespace = " " + JsonGetString(JsonParse("\"\\u0009\\u000d\\u000a\""));
    int i;
    for (i = 0; i < n; i++)
        if (FindSubString(whitespace, GetSubString(s, i, 1)) < 0) return TRUE;
    return FALSE;
}

// A render snapshot has no use for game prose. Native ObjectToJson strings can
// contain mixed encodings; strip GFF free text before a reader encodes the GFF.
// Keep resource references, raw metadata and every other (including future) field.
json MgLiveLookNoText(json fields)
{
    // Native UTC/UTI text slots are stable. Clear these first so ordinary looks
    // avoid an interpreted walk of hundreds of numeric appearance fields.
    json textKeys = JsonParse("[\"Deity\",\"Description\",\"DescriptionOverr\",\"DescIdentified\",\"DescIdentifiedOv\",\"FirstName\",\"LastName\",\"LocalizedName\",\"NWNX_POS\",\"Subrace\",\"Tag\",\"TextBubbleText\",\"TargetTag\"]");
    int k;
    for (k = 0; k < 13; k++)
    {
        string key = JsonGetString(JsonArrayGet(textKeys, k));
        string type = JsonGetString(JsonObjectGet(JsonObjectGet(fields, key), "type"));
        if (type == "cexostring" || type == "cexolocstring") JsonObjectDelInplace(fields, key);
    }
    // Both structs carry prose in ObjectToJson, including equipped item snapshots.
    for (k = 0; k < 2; k++)
    {
        string key = "MiscVisuals";
        string textKey = "TextBubbleText";
        if (k) { key = "Transition"; textKey = "TargetTag"; }
        json field = JsonObjectGet(fields, key);
        if (JsonGetString(JsonObjectGet(field, "type")) == "struct")
        {
            json value = JsonObjectGet(field, "value");
            string type = JsonGetString(JsonObjectGet(JsonObjectGet(value, textKey), "type"));
            if (type == "cexostring" || type == "cexolocstring")
            {
                JsonObjectDelInplace(value, textKey);
                JsonObjectSetInplace(field, "value", value);
                JsonObjectSetInplace(fields, key, field);
            }
        }
    }
    // shortcut: no walk of the whole tree (a PC's look ran out of NWScript instructions);
    // Moonglow drops any other free text when it reads the look (93dc6bc).
    return fields;
}

// A look describes models, parts and colours, not a second creature snapshot.
// Scan just the root and five visible equipment rows: never walk arbitrary lists.
// Preserve GFF metadata, EE wide twins and per-armour-part colour overrides.
json MgLiveLookFields(json fields, int item = FALSE)
{
    json keys = JsonObjectKeys(fields);
    int i;
    for (i = 0; i < JsonGetLength(keys); i++)
    {
        string key = JsonGetString(JsonArrayGet(keys, i));
        string lower = GetStringLowerCase(key);
        string part = lower;
        if (GetStringLeft(part, 1) == "x") part = GetSubString(part, 1, GetStringLength(part) - 1);
        int keep = lower == "__data_type" || lower == "__struct_id" ||
            FindSubString(part, "appearance") == 0 || FindSubString(part, "bodypart_") == 0 ||
            FindSubString(part, "armorpart_") == 0 || FindSubString(part, "color_") == 0;
        if (item)
            keep = keep || lower == "baseitem" || lower == "equippedres" || lower == "templateresref" ||
                lower == "hidden" || FindSubString(part, "modelpart") == 0 || FindSubString(lower, "apart_") == 0 ||
                FindSubString("|cloth1color|cloth2color|leather1color|leather2color|metal1color|metal2color|", "|" + lower + "|") >= 0;
        else
            keep = keep || lower == "equip_itemlist" || lower == "gender" || lower == "phenotype" ||
                FindSubString(part, "wings") == 0 || FindSubString(part, "tail") == 0;
        if (!keep) JsonObjectDelInplace(fields, key);
    }
    return fields;
}

// Keep GFF wrappers/metadata and future appearance fields, including EE wide twins.
json MgLiveLook(object pc)
{
    json look = ObjectToJson(pc);
    look = MgLiveLookFields(look);
    json keys;
    string dropped = "";
    int i;
    json equipment = JsonObjectGet(look, "Equip_ItemList");
    if (JsonGetType(equipment) == JSON_TYPE_OBJECT)
    {
        json items = JsonObjectGet(equipment, "value");
        json kept = JsonArray();
        for (i = 0; i < JsonGetLength(items); i++)
        {
            json item = JsonArrayGet(items, i);
            int slot = JsonGetInt(JsonObjectGet(item, "__struct_id"));
            if (slot == 1 || slot == 2 || slot == 16 || slot == 32 || slot == 64)
            {
                kept = JsonArrayInsert(kept, MgLiveLookFields(item, TRUE));
            }
        }
        look = JsonObjectSet(look, "Equip_ItemList", JsonObjectSet(equipment, "value", kept));
    }
    look = MgLiveLookNoText(look);
    // Unrecognized lists are retained unless measured large. Equipment is useful
    // and only sacrificed by the whole-look budget as a last resort.
    keys = JsonObjectKeys(look);
    for (i = 0; i < JsonGetLength(keys); i++)
    {
        string key = JsonGetString(JsonArrayGet(keys, i));
        json field = JsonObjectGet(look, key);
        if (key != "Equip_ItemList" && JsonGetString(JsonObjectGet(field, "type")) == "list" &&
            GetStringLength(JsonDump(field)) > 4096)
        {
            look = JsonObjectDel(look, key);
            dropped += key + ",";
        }
    }
    int bytes = GetStringLength(JsonDump(look));
    while (bytes > 32768)
    {
        string largest = "";
        int size = 0;
        keys = JsonObjectKeys(look);
        for (i = 0; i < JsonGetLength(keys); i++)
        {
            string key = JsonGetString(JsonArrayGet(keys, i));
            json field = JsonObjectGet(look, key);
            if (JsonGetString(JsonObjectGet(field, "type")) == "list")
            {
                int n = GetStringLength(JsonDump(field));
                if (n > size) { largest = key; size = n; }
            }
        }
        if (largest == "") break;
        look = JsonObjectDel(look, largest);
        dropped += largest + ",";
        bytes = GetStringLength(JsonDump(look));
    }
    // Accumulate any fallback removals until the next per-PC size log.
    string previous = GetLocalString(pc, "MGLIVE_LOOK_DROPPED");
    if (dropped != "" && FindSubString(previous, dropped) < 0)
        SetLocalString(pc, "MGLIVE_LOOK_DROPPED", GetStringLeft(previous + dropped, 1024));
    int tick = GetLocalInt(GetModule(), "MGLIVE_SEQ");
    if (tick >= GetLocalInt(pc, "MGLIVE_LOOK_LOG_AT"))
    {
        WriteTimestampedLogEntry("MGLIVE: look " + ObjectToString(pc) + " bytes=" + IntToString(bytes) +
            " dropped=" + GetLocalString(pc, "MGLIVE_LOOK_DROPPED"));
        SetLocalInt(pc, "MGLIVE_LOOK_LOG_AT", tick + 60);
        DeleteLocalString(pc, "MGLIVE_LOOK_DROPPED");
    }
    // A pathological scalar cannot be fixed by dropping lists. Keep the dot,
    // not a packet so large it rejects the entire area's live state.
    if (bytes > 32768) return JsonNull();
    return look;
}

// A five-slot signature detects every visible equip/unequip on the next tick.
string MgLiveLookSignature(object o)
{
    string signature = GetLocalString(GetModule(), "MGLIVE_BOOT") + ObjectToString(o) + ":" + IntToString(GetAppearanceType(o));
    int slot;
    for (slot = 0; slot <= 6; slot++)
        if (slot == 0 || slot == 1 || slot == 4 || slot == 5 || slot == 6)
            signature += ":" + ObjectToString(GetItemInSlot(slot, o));
    return signature;
}

// Preflight builds before the upcoming report (tick+1). Spread periodic
// refreshes across 29 phases instead of expiring a cold crowd together. The
// deadline stays 2..30 ticks ahead: strictly beyond that upcoming report, and
// no older than the existing 30-tick bound. Dirty equipment still bypasses it.
int MgLiveLookRefreshAt(object o, int tick)
{
    int phase=(HashString(ObjectToString(o)) & 0x7fffffff)%29;
    int earliest=tick+2;
    return earliest+(phase-earliest%29+29)%29;
}

// NPCs need the same native appearance data as PCs. Cache the compact look.
json MgLiveNpcLook(object o)
{
    object module = GetModule();
    string signature = MgLiveLookSignature(o);
    int tick = GetLocalInt(module, "MGLIVE_SEQ");
    if (GetLocalString(o, "MGLIVE_LOOK_SIGNATURE") != signature || tick >= GetLocalInt(o, "MGLIVE_LOOK_REFRESH"))
    {
        json look = MgLiveLook(o);
        // Absence is an authoritative empty inventory, never blueprint equipment.
        if (JsonGetType(JsonObjectGet(look, "Equip_ItemList")) != JSON_TYPE_OBJECT)
            JsonObjectSetInplace(look, "Equip_ItemList", JsonObjectSet(JsonObjectSet(JsonObject(), "type", JsonString("list")), "value", JsonArray()));
        SetLocalJson(o, "MGLIVE_NPC_LOOK", look);
        SetLocalInt(o, "MGLIVE_NPC_LOOK_HASH", HashString(JsonDump(look)));
        SetLocalString(o, "MGLIVE_LOOK_SIGNATURE", signature);
        SetLocalInt(o, "MGLIVE_LOOK_REFRESH", MgLiveLookRefreshAt(o, tick));
    }
    return GetLocalJson(o, "MGLIVE_NPC_LOOK");
}

// Sound objects have no NWScript OBJECT_TYPE_* bit. Use a private dispatch
// value after checking the NWNX internal type; never pass it to CreateObject.
const int MGLIVE_TYPE_SOUND=1024;
int MgLiveType(object o)
{
    int t=GetObjectType(o);
    if(t==OBJECT_TYPE_INVALID && NWNX_Object_GetInternalObjectType(o)==NWNX_OBJECT_TYPE_INTERNAL_SOUND) return MGLIVE_TYPE_SOUND;
    return t;
}

json MgLiveObject(object o, int bFull)
{
    json j = JsonObject();
    vector p = GetPosition(o);
    j = JsonObjectSet(j, "id", JsonString(ObjectToString(o)));
    j = MgLiveTextField(j, "tag", GetStringLeft(GetTag(o), 256));
    j = JsonObjectSet(j, "r", JsonString(GetResRef(o)));
    j = MgLiveTextField(j, "name", GetStringLeft(GetName(o), 256));
    j = JsonObjectSet(j, "x", JsonFloat(p.x));
    j = JsonObjectSet(j, "y", JsonFloat(p.y));
    j = JsonObjectSet(j, "z", JsonFloat(p.z));
    j = JsonObjectSet(j, "f", JsonFloat(GetFacing(o)));
    j = JsonObjectSet(j, "own", JsonBool(GetLocalInt(o, "MGLIVE_OWN")));
    int t = MgLiveType(o);
    if (t == OBJECT_TYPE_CREATURE || t == OBJECT_TYPE_PLACEABLE || t == OBJECT_TYPE_DOOR)
    {
        json hp = JsonArrayInsert(JsonArray(), JsonInt(GetCurrentHitPoints(o)));
        hp = JsonArrayInsert(hp, JsonInt(GetMaxHitPoints(o)));
        j = JsonObjectSet(j, "hp", hp);
    }
    if (t == OBJECT_TYPE_CREATURE)
    {
        MgLiveObserveDeath(o);
        j = JsonObjectSet(j, "anim", JsonInt(NWNX_Object_GetCurrentAnimation(o)));
        if(GetLocalInt(o,"MGLIVE_ANIM_ACTIVE")) j=JsonObjectSet(j,"animation",GetLocalJson(o,"MGLIVE_ANIMATION"));
        j = JsonObjectSet(j, "app", JsonInt(GetAppearanceType(o)));
        j = JsonObjectSet(j, "app_original", JsonInt(MgLiveOriginalAppearance(o, GetAppearanceType(o))));
        j = JsonObjectSet(j, "portrait", JsonString(GetPortraitResRef(o)));
        j = JsonObjectSet(j, "faction", JsonInt(NWNX_Creature_GetFaction(o)));
        j = JsonObjectSet(j, "gold", JsonInt(GetGold(o)));
        j = JsonObjectSet(j, "loot", JsonBool(GetLootable(o)));
        j = JsonObjectSet(j, "xp", JsonInt(GetXP(o)));
        j = JsonObjectSet(j, "level", JsonInt(GetHitDice(o)));
        // Internal type is required: the public effect type can hide VFX.
        // Engine proof: VFX_DUR_GLOW_RED stores row 411 at integer index 0.
        json vfx = JsonArray();
        int frozen = FALSE;
        effect visual = GetFirstEffect(o);
        while (GetIsEffectValid(visual))
        {
            if (GetEffectTag(visual) == "MGLIVE_FREEZE") frozen = TRUE;
            if (JsonGetLength(vfx) < 16 && GetEffectType(visual, TRUE) == EFFECT_TYPE_VISUALEFFECT &&
                GetEffectDurationType(visual) != DURATION_TYPE_INSTANT)
                vfx = JsonArrayInsert(vfx, JsonInt(GetEffectInteger(visual, 0)));
            visual = GetNextEffect(o);
        }
        // Empty is significant: it removes previously reported lasting effects.
        j = JsonObjectSet(j, "vfx", vfx);
        j = JsonObjectSet(j, "frozen", JsonBool(frozen));
        j = JsonObjectSet(j, "pc", JsonBool(GetIsPC(o)));
        j = JsonObjectSet(j, "dm", JsonBool(GetIsDM(o)));
        j = JsonObjectSet(j, "possessed", JsonBool(GetIsDMPossessed(o)));
        j = JsonObjectSet(j, "act", JsonInt(GetCurrentAction(o)));
        j = JsonObjectSet(j, "dead", JsonBool(GetIsDead(o)));
        int combat = GetIsInCombat(o);
        j = JsonObjectSet(j, "combat", JsonBool(combat));
        object target = GetAttackTarget(o);
        if (GetIsObjectValid(target)) j = JsonObjectSet(j, "target", JsonString(ObjectToString(target)));
        json hostile = JsonArray();
        json friendly = JsonArray();
        object pc = GetFirstPC();
        while (GetIsObjectValid(pc))
        {
            if (GetArea(pc) == GetArea(o))
            {
                if (GetIsEnemy(o, pc))
                    hostile = JsonArrayInsert(hostile, JsonString(ObjectToString(pc)));
                else if (GetIsFriend(o, pc))
                    friendly = JsonArrayInsert(friendly, JsonString(ObjectToString(pc)));
            }
            pc = GetNextPC();
        }
        j = JsonObjectSet(j, "hostile", hostile);
        j = JsonObjectSet(j, "friendly", friendly);
        if (!GetIsDM(o) && (!GetIsPC(o) || bFull))
        {
            json look;
            if (GetIsPC(o)) look = MgLiveLook(o);
            else look = MgLiveNpcLook(o);
            if (JsonGetType(look) == JSON_TYPE_OBJECT)
            {
                j = JsonObjectSet(j, "look", look);
                j = JsonObjectSet(j, "lookHash", JsonInt(GetIsPC(o) ? HashString(JsonDump(look)) : GetLocalInt(o, "MGLIVE_NPC_LOOK_HASH")));
            }
        }
    }
    if (bFull && (t == OBJECT_TYPE_PLACEABLE || t == OBJECT_TYPE_DOOR))
    {
        // Instance appearance can differ from the template (DM changes/custom GIT).
        int appearance;
        if (t == OBJECT_TYPE_PLACEABLE)
        {
            appearance = NWNX_Object_GetAppearance(o);
            j = JsonObjectSet(j, "static", JsonBool(NWNX_Object_GetPlaceableIsStatic(o)));
        }
        else
        {
            json snapshot = ObjectToJson(o);
            json generic = JsonObjectGet(snapshot, "GenericType_New");
            if (JsonGetType(generic) == JSON_TYPE_NULL) generic = JsonObjectGet(snapshot, "GenericType");
            appearance = JsonGetInt(JsonObjectGet(generic, "value"));
        }
        j = JsonObjectSet(j, "app", JsonInt(appearance));
        j = JsonObjectSet(j, "app_original", JsonInt(MgLiveOriginalAppearance(o, appearance)));
        j = JsonObjectSet(j, "open", JsonBool(GetIsOpen(o)));
        j = JsonObjectSet(j, "lock", JsonBool(GetLocked(o)));
        j = JsonObjectSet(j, "anim", JsonInt(NWNX_Object_GetCurrentAnimation(o)));
        // Native GetUseableFlag only reads placeables on this engine. Door
        // serialization exposes the actual common-object flag (engine-proved).
        int useable = GetUseableFlag(o);
        if (t == OBJECT_TYPE_DOOR)
            useable = JsonGetInt(JsonObjectGet(JsonObjectGet(ObjectToJson(o), "Useable"), "value"));
        j = JsonObjectSet(j, "useable", JsonBool(useable));
        float scale = GetObjectVisualTransform(o, OBJECT_VISUAL_TRANSFORM_SCALE);
        int changed = scale != 1.0;
        json rotate = JsonObject();
        json translate = JsonObject();
        int axis;
        for (axis = 0; axis < 3; axis++)
        {
            string key = GetSubString("xyz", axis, 1);
            float r = GetObjectVisualTransform(o, OBJECT_VISUAL_TRANSFORM_ROTATE_X + axis);
            float offset = GetObjectVisualTransform(o, OBJECT_VISUAL_TRANSFORM_TRANSLATE_X + axis);
            if (r != 0.0 || offset != 0.0) changed = TRUE;
            rotate = JsonObjectSet(rotate, key, JsonFloat(r));
            translate = JsonObjectSet(translate, key, JsonFloat(offset));
        }
        if (changed)
        {
            json vt = JsonObjectSet(JsonObject(), "scale", JsonFloat(scale));
            vt = JsonObjectSet(vt, "rotate", rotate);
            vt = JsonObjectSet(vt, "translate", translate);
            j = JsonObjectSet(j, "vt", vt);
        }
    }
    return j;
}

// Members are complete JsonDump values with a trailing comma. Build bytes,
// then parse once: JsonArrayInsert copies the entire growing JSON tree.
string MgLiveArrayText(string members)
{
    if (members != "") members = GetStringLeft(members, GetStringLength(members) - 1);
    return "[" + members + "]";
}

json MgLiveArea(object area, int bFull)
{
    json cre = JsonArray();
    json plc = JsonArray();
    json doors = JsonArray();
    json items = JsonArray();
    json waypoints = JsonArray();
    json sounds = JsonArray();
    json encounters = JsonArray();
    json stores = JsonArray();
    json triggers = JsonArray();

    object o = GetFirstObjectInArea(area);
    while (GetIsObjectValid(o))
    {
        int t = MgLiveType(o);
        if (t == OBJECT_TYPE_CREATURE)
            JsonArrayInsertInplace(cre, MgLiveObject(o, bFull));
        else if (bFull && t == OBJECT_TYPE_PLACEABLE)
            JsonArrayInsertInplace(plc, MgLiveObject(o, TRUE));
        else if (bFull && t == OBJECT_TYPE_DOOR)
            JsonArrayInsertInplace(doors, MgLiveObject(o, TRUE));
        else if (bFull && t == OBJECT_TYPE_ITEM)
            JsonArrayInsertInplace(items, MgLiveObject(o, TRUE));
        else if(bFull && t==OBJECT_TYPE_WAYPOINT) JsonArrayInsertInplace(waypoints,MgLiveObject(o,TRUE));
        else if(bFull && t==MGLIVE_TYPE_SOUND) JsonArrayInsertInplace(sounds,MgLiveObject(o,TRUE));
        else if(bFull && t==OBJECT_TYPE_ENCOUNTER) JsonArrayInsertInplace(encounters,MgLiveObject(o,TRUE));
        else if(bFull && t==OBJECT_TYPE_STORE) JsonArrayInsertInplace(stores,MgLiveObject(o,TRUE));
        else if(bFull && t==OBJECT_TYPE_TRIGGER) JsonArrayInsertInplace(triggers,MgLiveObject(o,TRUE));
        o = GetNextObjectInArea(area);
    }
    if(bFull)
    {
        // Native area iteration omits sounds. Track only explicitly spawned live
        // sounds, bounded at creation; prune destroyed objects on each full pass.
        json registered=GetLocalJson(area,"MGLIVE_SOUNDS");json retained=JsonArray();int si;
        for(si=0;si<JsonGetLength(registered);si++)
        {
            object sound=StringToObject(JsonGetString(JsonArrayGet(registered,si)));
            if(GetIsObjectValid(sound) && GetArea(sound)==area)
            {
                JsonArrayInsertInplace(retained,JsonString(ObjectToString(sound)));
                JsonArrayInsertInplace(sounds,MgLiveObject(sound,TRUE));
            }
        }
        SetLocalJson(area,"MGLIVE_SOUNDS",retained);
    }
    json result = JsonObject();
    JsonObjectSetInplace(result, "area", JsonString(GetResRef(area)));
    json size = JsonArray();
    JsonArrayInsertInplace(size, JsonInt(GetAreaSize(AREA_WIDTH, area)*10));
    JsonArrayInsertInplace(size, JsonInt(GetAreaSize(AREA_HEIGHT, area)*10));
    JsonObjectSetInplace(result, "size", size);
    JsonObjectSetInplace(result, "full", JsonBool(bFull));
    JsonObjectSetInplace(result, "cre", cre);
    if (bFull)
    {
        JsonObjectSetInplace(result, "plc", plc);
        JsonObjectSetInplace(result, "doors", doors);
        JsonObjectSetInplace(result, "items", items);
        JsonObjectSetInplace(result,"waypoints",waypoints);
        JsonObjectSetInplace(result,"sounds",sounds);
        JsonObjectSetInplace(result,"encounters",encounters);
        JsonObjectSetInplace(result,"stores",stores);
        JsonObjectSetInplace(result,"triggers",triggers);

    }
    return result;
}

// Selection by resref, never by tag; visit each area only once.
int MgLiveAreaWanted(object a)
{
    object module = GetModule();
    if(GetLocalInt(a,"MGLIVE_WATCH_UNTIL") > GetLocalInt(module,"MGLIVE_SEQ")) return TRUE;
    string forced = "," + GetStringLowerCase(GetLocalString(module, "MGLIVE_AREAS")) + ",";
    if (GetLocalInt(a, "MGLIVE_WATCHED") || FindSubString(forced, "," + GetStringLowerCase(GetResRef(a)) + ",") >= 0) return TRUE;
    object pc = GetFirstPC();
    while (GetIsObjectValid(pc))
    {
        if (GetArea(pc) == a) return TRUE;
        pc = GetNextPC();
    }
    return FALSE;
}

// Build cold/changed/expired NPC looks in separate VM invocations, at most
// four per continuation. No sequence is consumed and no partial report is
// published while warming. Re-scan live objects so destruction/area changes
// cannot strand a pending counter or a saved engine iterator.
int MgLiveLooksReady()
{
    int tick = GetLocalInt(GetModule(), "MGLIVE_SEQ") + 1;
    int queued = 0;
    object a = GetFirstArea();
    while (GetIsObjectValid(a))
    {
        if (MgLiveAreaWanted(a))
        {
            object o = GetFirstObjectInArea(a);
            while (GetIsObjectValid(o))
            {
                if (GetObjectType(o) == OBJECT_TYPE_CREATURE && !GetIsPC(o) && !GetIsDM(o) &&
                    (GetLocalString(o, "MGLIVE_LOOK_SIGNATURE") != MgLiveLookSignature(o) || tick >= GetLocalInt(o, "MGLIVE_LOOK_REFRESH")))
                {
                    DelayCommand(0.0, ExecuteScript("mglive_look", o));
                    if (++queued >= 4) return FALSE;
                }
                o = GetNextObjectInArea(a);
            }
        }
        a = GetNextArea();
    }
    return queued == 0;
}

json MgLiveAreas(int bFull)
{
    json areas = JsonArray();
    object a = GetFirstArea();
    while (GetIsObjectValid(a))
    {
        if (MgLiveAreaWanted(a)) JsonArrayInsertInplace(areas, MgLiveArea(a, bFull));
        a = GetNextArea();
    }
    return areas;
}

// Complete rows are hashed; only changed rows and vanished IDs cross Redis.
// Cache belongs to the module/area resref, so object destruction cannot erase it.
json MgLiveDeltaAreas(json areas, int full, int seq)
{
    object m = GetModule();
    string output = "";
    int ai;
    for (ai = 0; ai < JsonGetLength(areas); ai++)
    {
        json area = JsonArrayGet(areas, ai);
        string name = JsonGetString(JsonObjectGet(area, "area"));
        string key = "MGLIVE_HASH_" + name;
        json before = GetLocalJson(m, key);
        if (JsonGetType(before) != JSON_TYPE_OBJECT) before = JsonObject();
        json beforeLooks = GetLocalJson(m, "MGLIVE_LHASH_" + name);
        string lookHashes = "";
        string hashes = "";
        string collections = "";
        int areaFull = full || GetLocalInt(m, "MGLIVE_SEEN_" + name) != seq - 1;
        int k;
        for (k = 0; k < 9; k++)
        {
            string kind = "cre";
            if (k == 1) kind = "plc";
            else if (k == 2) kind = "doors";
            else if (k == 3) kind = "items";
            else if(k==4) kind="waypoints";
            else if(k==5) kind="sounds";
            else if(k==6) kind="encounters";
            else if(k==7) kind="stores";
            else if(k==8) kind="triggers";

            json rows = JsonObjectGet(area, kind);
            string changed = "";
            int i;
            for (i = 0; i < JsonGetLength(rows); i++)
            {
                json row = JsonArrayGet(rows, i);
                string id = JsonGetString(JsonObjectGet(row, "id"));
                int hash = HashString(JsonDump(row));
                json previous = JsonObjectGet(before, id);
                json lookHash = JsonObjectGet(row, "lookHash");
                int hasLook = k == 0 && JsonGetType(lookHash) == JSON_TYPE_INTEGER;
                if (hasLook) lookHashes += JsonDump(JsonString(id)) + ":" + JsonDump(lookHash) + ",";
                if (!areaFull && (JsonGetType(previous) != JSON_TYPE_INTEGER || JsonGetInt(previous) != hash))
                {
                    // The full row still defines its hash. A moving row can reuse
                    // an already delivered look; full/resync packets always carry it.
                    if (hasLook && JsonGetType(previous) == JSON_TYPE_INTEGER &&
                        JsonDump(JsonObjectGet(beforeLooks, id)) == JsonDump(lookHash))
                        row = JsonObjectDel(row, "look");
                    changed += JsonDump(row) + ",";
                }
                hashes += JsonDump(JsonString(id)) + ":" + IntToString(hash) + ",";
            }
            collections += ",\"" + kind + "\":";
            if (areaFull) collections += JsonDump(rows);
            else collections += MgLiveArrayText(changed);
        }
        if (hashes != "") hashes = GetStringLeft(hashes, GetStringLength(hashes) - 1);
        json after = JsonParse("{" + hashes + "}");
        string removed = "";
        if (!areaFull)
        {
            json ids = JsonObjectKeys(before);
            int i;
            for (i = 0; i < JsonGetLength(ids); i++)
            {
                string id = JsonGetString(JsonArrayGet(ids, i));
                if (JsonGetType(JsonObjectGet(after, id)) == JSON_TYPE_NULL)
                    removed += JsonDump(JsonString(id)) + ",";
            }
        }
        string data = "{\"area\":" + JsonDump(JsonString(name)) +
            ",\"size\":" + JsonDump(JsonObjectGet(area, "size")) + ",\"full\":";
        if (areaFull) data += "true";
        else data += "false";
        output += data + collections + ",\"removed\":" + MgLiveArrayText(removed) + "},";
        SetLocalJson(m, key, after);
        if (lookHashes != "") lookHashes = GetStringLeft(lookHashes, GetStringLength(lookHashes) - 1);
        SetLocalJson(m, "MGLIVE_LHASH_" + name, JsonParse("{" + lookHashes + "}"));
        SetLocalInt(m, "MGLIVE_SEEN_" + name, seq);
    }
    return JsonParse(MgLiveArrayText(output));
}

void MgLiveFailure(string reason)
{
    object m = GetModule();
    int tick = GetLocalInt(m, "MGLIVE_SEQ");
    if (tick >= GetLocalInt(m, "MGLIVE_LOG_AT"))
    {
        WriteTimestampedLogEntry("MGLIVE: " + reason);
        SetLocalInt(m, "MGLIVE_LOG_AT", tick + 60);
    }
}

// Defined in mglive_exec; only the pub/sub entry point reaches this handler.
json MgLiveRun(json j);

void MgLiveHandleReply(string message)
{
    object m = GetModule();
    json response = JsonParse(message);
    json commands = JsonObjectGet(response, "commands");
    if (JsonGetInt(JsonObjectGet(response, "v")) != 1 || JsonGetType(commands) != JSON_TYPE_ARRAY ||
        JsonGetLength(commands) > MGLIVE_MAX_DONE)
    {
        MgLiveFailure("invalid command message; acknowledgements retained");
        return;
    }
    MgLiveReceiveResults(JsonObjectGet(response, "received"));
    json ack = GetLocalJson(m, "MGLIVE_ACK");
    json results = GetLocalJson(m, "MGLIVE_RESULTS");
    json done = GetLocalJson(m, "MGLIVE_DONE");
    int i;
    for (i = 0; i < JsonGetLength(commands); i++)
    {
        json command = JsonArrayGet(commands, i);
        string id = JsonGetString(JsonObjectGet(command, "id"));
        if (GetStringLength(id) < 1 || GetStringLength(id) > 64) continue;
        if (!MgLiveContains(done, id))
        {
            // Leave a large capture unacknowledged until pending results drain.
            if (JsonGetString(JsonObjectGet(command, "op")) == "capture_object" &&
                GetStringLength(JsonDump(results)) > 786432) continue;
            string previousId=GetLocalString(m,"MGLIVE_COMMAND_ID");string previousBy=GetLocalString(m,"MGLIVE_COMMAND_BY");
            SetLocalString(m,"MGLIVE_COMMAND_ID",id);
            DelayCommand(0.0,MgLiveClearCommandContext(id));
            string by=JsonGetString(JsonObjectGet(command,"by"));if(GetStringLength(by)>64) by="";
            SetLocalString(m,"MGLIVE_COMMAND_BY",by);
            json result = MgLiveRun(command);
            if(by!="") result=MgLiveTextField(result,"by",by);
            SetLocalString(m,"MGLIVE_COMMAND_ID",previousId);SetLocalString(m,"MGLIVE_COMMAND_BY",previousBy);
            done = JsonArrayInsert(done, JsonString(id));
            if (JsonGetLength(done) > MGLIVE_MAX_DONE) done = JsonArrayDel(done, 0);
            MgLiveRetainResult(result);
            results = GetLocalJson(m, "MGLIVE_RESULTS");
            ack = GetLocalJson(m, "MGLIVE_ACK");
        }
        if (!MgLiveContains(ack, id)) ack = JsonArrayInsert(ack, JsonString(id));
    }
    SetLocalJson(m, "MGLIVE_DONE", done);
    SetLocalJson(m, "MGLIVE_ACK", ack);
    SetLocalJson(m, "MGLIVE_RESULTS", results);
}
