#include "mglive_inc"
#include "nwnx_creature"
#include "nwnx_object"
#include "mglive_nui"
#include "mglive_dm"
#include "mglive_pcstop"

#include "mglive_result"
#include "mglive_watch"
#include "mglive_reply"
#include "mglive_animrep"
#include "mglive_loot"
#include "mglive_economy"
#include "mglive_var"
#include "mglive_objjson"
#include "mglive_atmos"
#include "mglive_spawn"

// A generation prevents an older timeout from releasing a newer order.
void MgLiveReleaseHold(object actor, int generation)
{
    if (!GetIsObjectValid(actor) || GetLocalInt(actor, "MGLIVE_HELD") != generation) return;
    SetCommandable(TRUE, actor);
    DeleteLocalInt(actor, "MGLIVE_HELD");
    int faction = GetLocalInt(actor, "MGLIVE_FACTION");
    if (faction)
    {
        DeleteLocalInt(actor, "MGLIVE_FACTION");
        NWNX_Creature_SetFaction(actor, faction - 1);
    }
}

void MgLiveCancelAnimation(object actor);

void MgLiveCancelHold(object actor)
{
    MgLiveCancelAnimation(actor);
    int held = GetLocalInt(actor, "MGLIVE_HELD");
    if (held) MgLiveReleaseHold(actor, held);
    if (GetLocalInt(actor, "MGLIVE_PATROL_ACTIVE"))
    {
        DeleteLocalInt(actor, "MGLIVE_PATROL_ACTIVE");
        DeleteLocalJson(actor, "MGLIVE_PATROL_POINTS");
        SetCommandable(TRUE, actor);
        AssignCommand(actor, ClearAllActions(TRUE));
    }
}

#include "mglive_tp"

// Runs on the module, so death/removal of either creature cannot strand a hold.
void MgLiveWatchHold(object actor, object victim, int generation, int attack)
{
    if (!GetIsObjectValid(actor) || GetLocalInt(actor, "MGLIVE_HELD") != generation) return;
    if ((attack && (!GetIsObjectValid(victim) || GetIsDead(victim))) || GetIsDead(actor) ||
        GetCurrentAction(actor) == ACTION_INVALID)
    {
        MgLiveReleaseHold(actor, generation);
        return;
    }
    DelayCommand(0.25, MgLiveWatchHold(actor, victim, generation, attack));
}

void MgLiveHold(int hold, int generation)
{
    if (hold)
    {
        SetLocalInt(OBJECT_SELF, "MGLIVE_HELD", generation);
        ActionDoCommand(MgLiveReleaseHold(OBJECT_SELF, generation));
        SetCommandable(FALSE);
    }
}

// Exact calm cannot clear an unselected enemy's queue. Stock AI may requeue a
// cached attack even after friendship, so hold this actor until a new order or
// the friendship interval expires. Unlike movement holds there is no idle watcher.
void MgLiveCalmHold(int generation)
{
    ClearAllActions(TRUE);
    SetLocalInt(OBJECT_SELF, "MGLIVE_HELD", generation);
    SetCommandable(FALSE);
}

void MgLiveMove(location dest, int run, int hold, int generation)
{
    ClearAllActions(TRUE);
    ActionMoveToLocation(dest, run);
    MgLiveHold(hold, generation);
}

void MgLiveAttack(object victim, int hold, int generation)
{
    ClearAllActions(TRUE);
    ActionAttack(victim);
    MgLiveHold(hold, generation);
}

int MgLiveNewOrder(object actor)
{
    MgLiveCancelHold(actor);
    int generation = GetLocalInt(actor, "MGLIVE_HOLD_SEQ") + 1;
    if (generation <= 0) generation = 1;
    SetLocalInt(actor, "MGLIVE_HOLD_SEQ", generation);
    return generation;
}

#include "mglive_patrol"
#include "mglive_anim"
#include "mglive_items"

// Names are standard factions; integers are raw module faction IDs, not the
// STANDARD_FACTION_* enum (whose values are offset from the usual module IDs).
int MgLiveFaction(json value)
{
    if (JsonGetType(value) == JSON_TYPE_INTEGER)
    {
        int n = JsonGetInt(value);
        if (MgLiveInt(value, 0, 65535)) return n;
    }
    if (JsonGetType(value) == JSON_TYPE_STRING)
    {
        string name = JsonGetString(value);
        if (name == "hostile") return 1;
        if (name == "commoner") return 2;
        if (name == "merchant") return 3;
        if (name == "defender") return 4;
    }
    return -1;
}

void MgLiveUnfreeze(object actor)
{
    effect frozen = GetFirstEffect(actor);
    while (GetIsEffectValid(frozen))
    {
        if (GetEffectTag(frozen) == "MGLIVE_FREEZE") RemoveEffect(actor, frozen);
        frozen = GetNextEffect(actor);
    }
}

int MgLiveTransformVector(json value, float limit)
{
    if (JsonGetType(value) != JSON_TYPE_OBJECT) return FALSE;
    int i;
    for (i = 0; i < 3; i++)
    {
        json part = JsonObjectGet(value, GetSubString("xyz", i, 1));
        if (!MgLiveNumber(part) || fabs(JsonGetFloat(part)) > limit) return FALSE;
    }
    return TRUE;
}

// Visual effects carry one private tag; unrelated script effects are untouched.

json MgLiveApplyVfx(json j, object area, object target = OBJECT_INVALID)
{
    string id = JsonGetString(JsonObjectGet(j, "id"));
    json row = JsonObjectGet(j, "row");
    if (!MgLiveInt(row, 0, 65535)) return MgLiveResult(id, "row must be integer 0-65535");
    string label = Get2DAString("visualeffects", "Label", JsonGetInt(row));
    if (label == "" || label == "****") return MgLiveResult(id, "visual effect row not found", OBJECT_INVALID, "not_found");
    string duration = JsonGetString(JsonObjectGet(j, "duration"));
    int type = DURATION_TYPE_INSTANT;
    float seconds = 0.0;
    if (duration == "temporary")
    {
        json sec = JsonObjectGet(j, "seconds");
        if (!MgLiveNumber(sec) || JsonGetFloat(sec) <= 0.0 || JsonGetFloat(sec) > 86400.0)
            return MgLiveResult(id, "temporary seconds must be >0 and <=86400");
        type = DURATION_TYPE_TEMPORARY;
        seconds = JsonGetFloat(sec);
    }
    else
    {
        if (duration == "permanent") type = DURATION_TYPE_PERMANENT;
        else if (duration != "instant") return MgLiveResult(id, "duration must be instant, temporary or permanent");
        if (JsonGetType(JsonObjectGet(j, "seconds")) != JSON_TYPE_NULL) return MgLiveResult(id, "seconds is only for temporary duration");
    }
    if (type != DURATION_TYPE_INSTANT && Get2DAString("visualeffects", "Type_FD", JsonGetInt(row)) != "D")
        return MgLiveResult(id, "lasting duration requires a duration visual effect row");
    json scaleValue=JsonObjectGet(j,"scale");float scale=1.0;
    if(JsonGetType(scaleValue)!=JSON_TYPE_NULL)
    { if(!MgLiveNumber(scaleValue) || JsonGetFloat(scaleValue)<0.01 || JsonGetFloat(scaleValue)>100.0) return MgLiveResult(id,"scale must be 0.01-100"); scale=JsonGetFloat(scaleValue); }
    json offsetValue=JsonObjectGet(j,"offset");json rotationValue=JsonObjectGet(j,"rotation");
    vector offset;vector rotation;
    if(JsonGetType(offsetValue)!=JSON_TYPE_NULL)
    { if(!MgLiveTransformVector(offsetValue,1000.0)) return MgLiveResult(id,"offset requires xyz within +/-1000"); offset=Vector(JsonGetFloat(JsonObjectGet(offsetValue,"x")),JsonGetFloat(JsonObjectGet(offsetValue,"y")),JsonGetFloat(JsonObjectGet(offsetValue,"z"))); }
    if(JsonGetType(rotationValue)!=JSON_TYPE_NULL)
    { if(!MgLiveTransformVector(rotationValue,3600.0)) return MgLiveResult(id,"rotation requires xyz within +/-3600"); rotation=Vector(JsonGetFloat(JsonObjectGet(rotationValue,"x")),JsonGetFloat(JsonObjectGet(rotationValue,"y")),JsonGetFloat(JsonObjectGet(rotationValue,"z"))); }
    effect visual = TagEffect(SupernaturalEffect(EffectVisualEffect(JsonGetInt(row),FALSE,scale,offset,rotation)), "MGLIVE_VFX");
    if (GetIsObjectValid(target)) ApplyEffectToObject(type, visual, target, seconds);
    else
    {
        json point = JsonObjectGet(j, "location");
        if (!MgLivePosition(point)) return MgLiveResult(id, "location requires finite xyz");
        ApplyEffectAtLocation(type, visual, Location(area, Vector(JsonGetFloat(JsonObjectGet(point, "x")),
            JsonGetFloat(JsonObjectGet(point, "y")), JsonGetFloat(JsonObjectGet(point, "z"))), 0.0), seconds);
    }
    json value = JsonObjectSet(JsonObject(), "row", row);
    value = JsonObjectSet(value, "duration", JsonString(duration));
    value = JsonObjectSet(value,"scale",JsonFloat(scale));
    if(JsonGetType(offsetValue)!=JSON_TYPE_NULL) value=JsonObjectSet(value,"offset",offsetValue);
    if(JsonGetType(rotationValue)!=JSON_TYPE_NULL) value=JsonObjectSet(value,"rotation",rotationValue);
    return JsonObjectSet(MgLiveResult(id), "value", value);
}

// AssignCommand executes after the caller returns. Capture this exact order's
// identity in its action closure, never infer it from speaker/text/timing.
void MgLiveSpeak(string line,int volume,string id,string by)
{
    object m=GetModule();string previousId=GetLocalString(m,"MGLIVE_COMMAND_ID");string previousBy=GetLocalString(m,"MGLIVE_COMMAND_BY");
    SetLocalString(m,"MGLIVE_COMMAND_ID",id);SetLocalString(m,"MGLIVE_COMMAND_BY",by);
    AssignCommand(m,DelayCommand(0.0,MgLiveClearCommandContext(id)));
    SpeakString(line,volume);
    SetLocalString(m,"MGLIVE_COMMAND_ID",previousId);SetLocalString(m,"MGLIVE_COMMAND_BY",previousBy);
}

json MgLiveRun(json j)
{
    string id = JsonGetString(JsonObjectGet(j, "id"));
    if (JsonGetType(j) != JSON_TYPE_OBJECT || GetStringLength(id) < 1 || GetStringLength(id) > 64)
        return MgLiveResult(id, "invalid command id");
    string op = JsonGetString(JsonObjectGet(j, "op"));
    // Transport control is module-wide and does not need an area/target.
    if (op == "resync")
    {
        SetLocalInt(GetModule(), "MGLIVE_RESYNC", TRUE);
        return MgLiveResult(id);
    }
    if (op == "watch_areas") return MgLiveWatchAreas(j);
    int playerExtra = op == "reply_to_player" || op == "pc_message" || op == "pc_journal" || op == "pc_take_item" || op == "pc_boot" || op == "fade" || op == "play_sound";
    int playerOp = playerExtra || op == "skill_check" || op == "pc_heal" || op == "pc_raise" || op == "pc_teleport" ||
        op == "pc_give_gold" || op == "pc_give_xp" || op == "pc_give_item" || op == "jump_all_players" || op == "give_level" || op == "give_alignment" || op == "set_stat";
    int reputationOp = op == "get_faction_reputation" || op == "set_faction_reputation";
    int serverPlayers = GetLocalString(GetModule(), "MGLIVE_ALLOW_PLAYER_OPS") == "1";
    if (playerOp && !serverPlayers) return MgLiveResult(id, "player operations disabled on server", OBJECT_INVALID, "disabled");
    json permission = JsonObjectGet(j, "players");
    if (JsonGetType(permission) != JSON_TYPE_NULL && JsonGetType(permission) != JSON_TYPE_BOOL)
        return MgLiveResult(id, "players must be boolean");
    int allowPlayers = serverPlayers && JsonGetType(permission) == JSON_TYPE_BOOL && JsonGetInt(permission);
    if (playerOp && !allowPlayers) return MgLiveResult(id, "players flag required", OBJECT_INVALID, "disabled");
    if (op == "reply_to_player") return MgLiveReplyToPlayer(j);
    if (op == "teleport" || op == "pc_teleport") return MgLiveTeleport(j, allowPlayers, op == "pc_teleport");
    object area = MgLiveFindArea(JsonGetString(JsonObjectGet(j, "area")));
    if (!GetIsObjectValid(area)) return MgLiveResult(id, "unknown area", OBJECT_INVALID, "not_found");
    if(op=="set_weather" || op=="set_fog" || op=="set_area_light" || op=="set_skybox" || op=="set_music" || op=="set_ambient_sound" || op=="sound_at") return MgLiveAtmosphere(j,area);
    if (op == "set_time" || op == "set_date")
    {
        if (op == "set_time")
        {
            json hour = JsonObjectGet(j, "hour"); json minute = JsonObjectGet(j, "minute");
            if (!MgLiveInt(hour, 0, 23) || !MgLiveInt(minute, 0, 59))
                return MgLiveResult(id, "hour 0-23 and minute 0-59 required");
            // Native minute/second fields are real-time offsets within the
            // module's compressed hour, despite nwscript.nss saying 0..59.
            int minutesPerHour = NWNX_Util_GetMinutesPerHour();
            if (minutesPerHour < 1) return MgLiveResult(id, "invalid module time scale");
            int seconds = JsonGetInt(minute) * minutesPerHour;
            SetTime(JsonGetInt(hour), seconds / 60, seconds % 60, 0);
        }
        else
        {
            json year = JsonObjectGet(j, "year"); json month = JsonObjectGet(j, "month"); json day = JsonObjectGet(j, "day");
            if (!MgLiveInt(year, 0, 32000) || !MgLiveInt(month, 1, 12) || !MgLiveInt(day, 1, 28))
                return MgLiveResult(id, "year 0-32000, month 1-12, day 1-28 required");
            int requestedDate = JsonGetInt(year)*336 + (JsonGetInt(month)-1)*28 + JsonGetInt(day);
            int currentDate = GetCalendarYear()*336 + (GetCalendarMonth()-1)*28 + GetCalendarDay();
            if (requestedDate < currentDate) return MgLiveResult(id, "native calendar cannot move backwards");
            SetCalendar(JsonGetInt(year), JsonGetInt(month), JsonGetInt(day));
        }
        return MgLiveResult(id);
    }
    if (op == "set_tile")
    {
        json x = JsonObjectGet(j, "x"); json y = JsonObjectGet(j, "y");
        json tile = JsonObjectGet(j, "tile_id"); json orientation = JsonObjectGet(j, "orientation"); json height = JsonObjectGet(j, "height");
        if (!MgLiveInt(x, 0, 65535) || !MgLiveInt(y, 0, 65535) || !MgLiveInt(tile, 0, 65535) ||
            !MgLiveInt(orientation, 0, 3) || !MgLiveInt(height, 0, 255)) return MgLiveResult(id, "tile coordinates/id 0..65535, orientation 0..3 and height 0..255 must be integers");
        int width = GetAreaSize(AREA_WIDTH, area); int length = GetAreaSize(AREA_HEIGHT, area);
        if (JsonGetInt(x) >= width || JsonGetInt(y) >= length) return MgLiveResult(id, "tile coordinates outside area");
        string tileset = GetStringLowerCase(ResManGetFileContents(GetTilesetResRef(area), RESTYPE_SET));
        if (tileset == "") return MgLiveResult(id, "tileset resource unavailable", OBJECT_INVALID, "not_found");
        // Check the loaded resource, including overrides/HAKs, before invoking SetTile.
        string section = "[tile" + IntToString(JsonGetInt(tile)) + "]";
        if (FindSubString(tileset, section) < 0) return MgLiveResult(id, "tile id is absent from the active tileset", OBJECT_INVALID, "not_found");
        location at = Location(area, Vector(IntToFloat(JsonGetInt(x))*10.0+5.0, IntToFloat(JsonGetInt(y))*10.0+5.0, 0.0), 0.0);
        json before = JsonObjectSet(JsonObject(), "tile_id", JsonInt(GetTileID(at)));
        before = JsonObjectSet(before, "orientation", JsonInt(GetTileOrientation(at)));
        before = JsonObjectSet(before, "height", JsonInt(GetTileHeight(at)));
        int flags = SETTILE_FLAG_RECOMPUTE_LIGHTING | SETTILE_FLAG_RELOAD_GRASS;
        if (JsonGetInt(x) == 0 || JsonGetInt(y) == 0 || JsonGetInt(x) == width-1 || JsonGetInt(y) == length-1)
            flags = flags | SETTILE_FLAG_RELOAD_BORDER;
        SetTile(at, JsonGetInt(tile), JsonGetInt(orientation), JsonGetInt(height), flags);
        json value = JsonObjectSet(JsonObject(), "x", x);
        value = JsonObjectSet(value, "y", y);
        value = JsonObjectSet(value, "tile_id", JsonInt(GetTileID(at)));
        value = JsonObjectSet(value, "orientation", JsonInt(GetTileOrientation(at)));
        value = JsonObjectSet(value, "height", JsonInt(GetTileHeight(at)));
        value = JsonObjectSet(value, "before", before);
        value = JsonObjectSet(value, "flags", JsonInt(flags));
        if (GetTileID(at) != JsonGetInt(tile) || GetTileOrientation(at) != JsonGetInt(orientation) || GetTileHeight(at) != JsonGetInt(height))
            return JsonObjectSet(MgLiveResult(id, "tile readback did not match requested change"), "value", value);
        json e = MgLiveEvent("tile");
        e = JsonObjectSet(e, "area", JsonString(GetResRef(area)));
        json keys = JsonObjectKeys(value); int n;
        for (n = 0; n < JsonGetLength(keys); n++)
        {
            string key = JsonGetString(JsonArrayGet(keys, n));
            e = JsonObjectSet(e, key, JsonObjectGet(value, key));
        }
        MgLiveQueueEvent(e);
        return JsonObjectSet(MgLiveResult(id), "value", value);
    }
    if (op == "skill_check")
    {
        if(GetLocalInt(GetModule(),"MGLIVE_SKILL_ACTIVE")>=16) return MgLiveResult(id,"at most 16 skill requests may await rolls");
        json x = JsonObjectGet(j, "x"); json y = JsonObjectGet(j, "y"); json radius = JsonObjectGet(j, "radius");
        json skill = JsonObjectGet(j, "skill"); json dc = JsonObjectGet(j, "dc");
        json bonusValue=JsonObjectGet(j,"bonus"); json timeoutValue=JsonObjectGet(j,"timeout"); json showValue=JsonObjectGet(j,"show_dc");
        if ((JsonGetType(bonusValue)!=JSON_TYPE_NULL && !MgLiveInt(bonusValue,-100,100)) ||
            (JsonGetType(timeoutValue)!=JSON_TYPE_NULL && !MgLiveInt(timeoutValue,1,600)) ||
            (JsonGetType(showValue)!=JSON_TYPE_NULL && JsonGetType(showValue)!=JSON_TYPE_BOOL)) return MgLiveResult(id,"bonus -100..100, timeout 1..600 and boolean show_dc required");
        int bonus=JsonGetInt(bonusValue); int timeout=120; int showDC=TRUE;
        if(JsonGetType(timeoutValue)!=JSON_TYPE_NULL)timeout=JsonGetInt(timeoutValue);
        if(JsonGetType(showValue)!=JSON_TYPE_NULL)showDC=JsonGetInt(showValue);
        json target = JsonObjectGet(j, "target");
        int single = JsonGetType(target) != JSON_TYPE_NULL;
        if (single && (JsonGetType(target) != JSON_TYPE_STRING || JsonGetType(x) != JSON_TYPE_NULL || JsonGetType(y) != JSON_TYPE_NULL || JsonGetType(radius) != JSON_TYPE_NULL)) return MgLiveResult(id, "supply target or xy/radius exclusively");
        if ((!single && (!MgLiveNumber(x) || !MgLiveNumber(y) || !MgLiveNumber(radius) || JsonGetFloat(radius) <= 0.0 || JsonGetFloat(radius) > 30.0)) ||
            !MgLiveInt(skill, 0, 65535) || !MgLiveInt(dc, 0, 255)) return MgLiveResult(id, "finite xy, radius >0..30, skill row and DC 0..255 required");
        string label = Get2DAString("skills", "Label", JsonGetInt(skill));
        if (label == "" || label == "****") return MgLiveResult(id, "skill row not found", OBJECT_INVALID, "not_found");
        json success = JsonObjectGet(j, "success_text"); json failure = JsonObjectGet(j, "fail_text");
        if ((JsonGetType(success) != JSON_TYPE_NULL && JsonGetType(success) != JSON_TYPE_STRING) ||
            (JsonGetType(failure) != JSON_TYPE_NULL && JsonGetType(failure) != JSON_TYPE_STRING)) return MgLiveResult(id, "messages must be strings");
        string yes = JsonGetString(success); string no = JsonGetString(failure);
        if ((yes != "" && !MgLiveText(yes)) || (no != "" && !MgLiveText(no))) return MgLiveResult(id, "messages must be empty or 1-200 nonblank characters");
        json rows = JsonArray(); int truncated = FALSE;
        object pc = GetFirstPC();
        if (single)
        {
            if (!MgLiveObjectId(JsonGetString(target))) return MgLiveResult(id, "invalid target id");
            pc = StringToObject(JsonGetString(target));
            if (!GetIsObjectValid(pc)) return MgLiveResult(id, "target not found", OBJECT_INVALID, "not_found");
            if (GetArea(pc) != area) return MgLiveResult(id, "target in another area", OBJECT_INVALID, "other_area");
            if (!GetIsPC(pc) || GetIsDM(pc) || GetIsDMPossessed(pc)) return MgLiveResult(id, "target must be a PC, not a DM", OBJECT_INVALID, "protected");
            if (GetIsDead(pc)) return MgLiveResult(id, "player is dead", OBJECT_INVALID, "dead");
        }
        while (GetIsObjectValid(pc))
        {
            vector p = GetPosition(pc);
            float dx = p.x - JsonGetFloat(x); float dy = p.y - JsonGetFloat(y); float r = JsonGetFloat(radius);
            if (GetIsPC(pc) && !GetIsDM(pc) && !GetIsDMPossessed(pc) && !GetIsDead(pc) && GetArea(pc) == area && (single || dx*dx+dy*dy <= r*r))
            {
                if (JsonGetLength(rows) >= 64) { truncated = TRUE; break; }
                rows = JsonArrayInsert(rows, MgLiveSkillInvite(pc,id,JsonGetInt(skill),JsonGetInt(dc),bonus,showDC));
            }
            if (single) break;
            pc = GetNextPC();
        }
        return MgLiveSkillBegin(id,rows,JsonGetInt(skill),JsonGetInt(dc),bonus,showDC,timeout,yes,no,truncated);
    }
    if (op == "jump_all_players")
    {
        if (!MgLivePosition(j)) return MgLiveResult(id, "finite coordinates required");
        location dest = Location(area, Vector(JsonGetFloat(JsonObjectGet(j, "x")), JsonGetFloat(JsonObjectGet(j, "y")), JsonGetFloat(JsonObjectGet(j, "z"))), 0.0);
        json affected = JsonArray();
        object pc = GetFirstPC();
        while (GetIsObjectValid(pc))
        {
            if (GetIsPC(pc) && !GetIsDM(pc) && !GetIsDMPossessed(pc) && !GetIsDead(pc))
                affected = JsonArrayInsert(affected, JsonString(ObjectToString(pc)));
            pc = GetNextPC();
        }
        int i;
        for (i = 0; i < JsonGetLength(affected); i++)
        {
            pc = StringToObject(JsonGetString(JsonArrayGet(affected, i)));
            MgLiveCancelHold(pc);
            AssignCommand(pc, ClearAllActions(TRUE));
            AssignCommand(pc, JumpToLocation(dest));
        }
        return JsonObjectSet(MgLiveResult(id), "affected", affected);
    }
    if (op == "get_variable" || op == "set_variable" || op == "dump_locals")
        return MgLiveVariables(j, area, allowPlayers);
    if (op == "stop_combat")
    {
        json requested = JsonObjectGet(j, "target");
        json x = JsonObjectGet(j, "x"); json y = JsonObjectGet(j, "y"); json r = JsonObjectGet(j, "radius");
        json affected = JsonArray();
        object actor;
        if (JsonGetType(requested) != JSON_TYPE_NULL)
        {
            if (JsonGetType(x) != JSON_TYPE_NULL || JsonGetType(y) != JSON_TYPE_NULL || JsonGetType(r) != JSON_TYPE_NULL)
                return MgLiveResult(id, "use target or x/y/radius, never both");
            if (JsonGetType(requested) != JSON_TYPE_STRING || !MgLiveObjectId(JsonGetString(requested)))
                return MgLiveResult(id, "invalid target id");
            actor = MgLiveFindObject(JsonGetString(requested));
            if (!GetIsObjectValid(actor)) return MgLiveResult(id, "target not found", OBJECT_INVALID, "not_found");
            if (GetArea(actor) != area) return MgLiveResult(id, "target in another area", OBJECT_INVALID, "other_area");
            if (GetObjectType(actor) != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target must be a creature");
            if (GetIsDead(actor)) return MgLiveResult(id, "target is dead", OBJECT_INVALID, "dead");
            if (GetIsPC(actor) && !GetIsDM(actor) && !GetIsDMPossessed(actor))
            {
                if (!allowPlayers) return MgLiveResult(id, "player stop requires both player switches", OBJECT_INVALID, "disabled");
                MgLiveStopPlayerAttack(actor);
                affected = JsonArrayInsert(affected, JsonString(ObjectToString(actor)));
                return JsonObjectSet(MgLiveResult(id), "affected", affected);
            }
            if (MgLiveDirectPlayer(actor) || (MgLiveProtected(actor) && !allowPlayers))
                return MgLiveResult(id, "target is protected", OBJECT_INVALID, "protected");
            affected = JsonArrayInsert(affected, JsonString(ObjectToString(actor)));
        }
        else
        {
            if (!MgLiveNumber(x) || !MgLiveNumber(y) || !MgLiveNumber(r))
                return MgLiveResult(id, "use target or all of finite x/y/radius");
            float radius = JsonGetFloat(r);
            if (radius <= 0.0 || radius > 50.0) return MgLiveResult(id, "radius must be greater than 0 and at most 50");
            vector center = Vector(JsonGetFloat(x), JsonGetFloat(y), 0.0);
            actor = GetFirstObjectInArea(area);
            while (GetIsObjectValid(actor))
            {
                vector p = GetPosition(actor); p.z = 0.0;
                if (GetObjectType(actor) == OBJECT_TYPE_CREATURE && !GetIsDead(actor) &&
                    !MgLiveDirectPlayer(actor) && (!MgLiveProtected(actor) || allowPlayers) && VectorMagnitude(p - center) <= radius)
                    affected = JsonArrayInsert(affected, JsonString(ObjectToString(actor)));
                actor = GetNextObjectInArea(area);
            }
        }
        // Area iterators are shared VM state: snapshot selection before the pair loop.
        int selected;
        for (selected = 0; selected < JsonGetLength(affected); selected++)
        {
                actor = StringToObject(JsonGetString(JsonArrayGet(affected, selected)));
                MgLiveCancelHold(actor);
                // SurrenderToEnemies also clears nearby PCs' actions and has a fixed
                // 10m radius. Calm only the selected NPCs with personal reputation.
                object other = GetFirstObjectInArea(area);
                while (GetIsObjectValid(other))
                {
                    if (other != actor && GetObjectType(other) == OBJECT_TYPE_CREATURE)
                    {
                        // This engine mutates the FIRST object in personal-reputation
                        // calls. TemporaryNeutral leaves a base-hostile pair at zero;
                        // a decaying friendship provides the required calm instead.
                        ClearPersonalReputation(actor, other);
                        SetIsTemporaryFriend(actor, other, TRUE, 180.0);
                    }
                    other = GetNextObjectInArea(area);
                }
                if (JsonGetType(requested) != JSON_TYPE_NULL)
                {
                    int generation = MgLiveNewOrder(actor);
                    AssignCommand(actor, MgLiveCalmHold(generation));
                    DelayCommand(180.0, MgLiveReleaseHold(actor, generation));
                }
                else AssignCommand(actor, ClearAllActions(TRUE));
        }
        return JsonObjectSet(MgLiveResult(id), "affected", affected);
    }
    if(op=="spawn_trigger") return MgLiveSpawnTrigger(j,area);
    if (op == "spawn")
    {
        int count = 1;
        json requestedCount = JsonObjectGet(j, "count");
        if (JsonGetType(requestedCount) != JSON_TYPE_NULL)
        {
            if (JsonGetType(requestedCount) != JSON_TYPE_INTEGER) return MgLiveResult(id, "count must be an integer");
            count = JsonGetInt(requestedCount);
            if (count < 1 || count > 10) return MgLiveResult(id, "count must be 1-10");
        }
        string r = JsonGetString(JsonObjectGet(j, "r"));
        if (!MgLiveResRef(r)) return MgLiveResult(id, "invalid resref");
        if (!MgLivePosition(j) || !MgLiveNumber(JsonObjectGet(j, "f")))
            return MgLiveResult(id, "invalid coordinates");
        string kind = JsonGetString(JsonObjectGet(j, "kind"));
        int type = OBJECT_TYPE_INVALID;
        if (kind == "creature") type = OBJECT_TYPE_CREATURE;
        else if (kind == "placeable") type = OBJECT_TYPE_PLACEABLE;
        else if (kind == "item") type = OBJECT_TYPE_ITEM;
        else if (kind == "waypoint") type = OBJECT_TYPE_WAYPOINT;
        else if (kind == "sound") type = MGLIVE_TYPE_SOUND;
        else if (kind == "encounter") type = OBJECT_TYPE_ENCOUNTER;
        else if (kind == "store") type = OBJECT_TYPE_STORE;
        else if (kind == "trigger") type = OBJECT_TYPE_TRIGGER;
        if (type == OBJECT_TYPE_INVALID) return MgLiveResult(id, "invalid spawn kind");
        json requestedFaction = JsonObjectGet(j, "faction");
        int faction = -1;
        if (JsonGetType(requestedFaction) != JSON_TYPE_NULL)
        {
            faction = MgLiveFaction(requestedFaction);
            if (type != OBJECT_TYPE_CREATURE || faction < 0) return MgLiveResult(id, "invalid creature faction");
        }
        int resource = RESTYPE_UTC;
        if (type == OBJECT_TYPE_PLACEABLE) resource = RESTYPE_UTP;
        if (type == OBJECT_TYPE_ITEM) resource = RESTYPE_UTI;
        if (type == OBJECT_TYPE_WAYPOINT) resource = RESTYPE_UTW;
        if (type == MGLIVE_TYPE_SOUND) resource = RESTYPE_UTS;
        if (type == OBJECT_TYPE_ENCOUNTER) resource = RESTYPE_UTE;
        if (type == OBJECT_TYPE_STORE) resource = RESTYPE_UTM;
        if (type == OBJECT_TYPE_TRIGGER) resource = RESTYPE_UTT;

        if (ResManGetAliasFor(r, resource) == "") return MgLiveResult(id, "blueprint not found", OBJECT_INVALID, "not_found");
        vector p = Vector(JsonGetFloat(JsonObjectGet(j, "x")), JsonGetFloat(JsonObjectGet(j, "y")), JsonGetFloat(JsonObjectGet(j, "z")));
        if(type==MGLIVE_TYPE_SOUND && JsonGetLength(GetLocalJson(area,"MGLIVE_SOUNDS"))+count>256) return MgLiveResult(id,"at most 256 live sound objects per area");
        json made = JsonArray();
        int columns = count; if (columns > 3) columns = 3;
        int rows = (count + columns - 1) / columns;
        int n;
        for (n = 0; n < count; n++)
        {
            vector at = p;
            if (count > 1)
            {
                at.x += (IntToFloat(n % columns) - IntToFloat(columns - 1) / 2.0) * 2.0;
                at.y += (IntToFloat(n / columns) - IntToFloat(rows - 1) / 2.0) * 2.0;
            }
            object created;
            location dest=Location(area,at,JsonGetFloat(JsonObjectGet(j,"f")));
            if(type==OBJECT_TYPE_CREATURE || type==OBJECT_TYPE_PLACEABLE || type==OBJECT_TYPE_ITEM) created=CreateObject(type,r,dest);
            else if(type==MGLIVE_TYPE_SOUND) created=NWNX_Area_CreateSoundObject(area,at,r);
            else
            {
                json source=TemplateToJson(r,resource);
                if(type==OBJECT_TYPE_TRIGGER || type==OBJECT_TYPE_ENCOUNTER) source=MgLiveSpawnFootprint(source);
                created=JsonToObject(source,dest);
            }
            int valid = GetIsObjectValid(created);
            if (valid && faction >= 0)
            {
                NWNX_Creature_SetFaction(created, faction);
                valid = NWNX_Creature_GetFaction(created) == faction;
            }
            if (!valid)
            {
                if (GetIsObjectValid(created)) DestroyObject(created);
                int rollback;
                for (rollback = 0; rollback < JsonGetLength(made); rollback++)
                    DestroyObject(StringToObject(JsonGetString(JsonArrayGet(made, rollback))));
                return MgLiveResult(id, "creation or faction assignment failed; batch removed");
            }
            SetLocalInt(created, "MGLIVE_OWN", TRUE);
            if(type==MGLIVE_TYPE_SOUND)
            {
                json registered=GetLocalJson(area,"MGLIVE_SOUNDS");if(JsonGetType(registered)!=JSON_TYPE_ARRAY) registered=JsonArray();
                SetLocalJson(area,"MGLIVE_SOUNDS",JsonArrayInsert(registered,JsonString(ObjectToString(created))));
            }
            made = JsonArrayInsert(made, JsonString(ObjectToString(created)));
        }
        json result = MgLiveResult(id, "", StringToObject(JsonGetString(JsonArrayGet(made, 0))));
        result = JsonObjectSet(result, "objects", made);
        return JsonObjectSet(result, "count", JsonInt(count));
    }

    if (op == "apply_vfx")
    {
        int hasTarget = JsonGetType(JsonObjectGet(j, "target")) != JSON_TYPE_NULL;
        int hasLocation = JsonGetType(JsonObjectGet(j, "location")) != JSON_TYPE_NULL;
        if (hasTarget == hasLocation) return MgLiveResult(id, "supply target or location exclusively");
        if (hasLocation) return MgLiveApplyVfx(j, area);
    }

    if (op == "spawn_json")
    {
        json data = JsonObjectGet(j, "json");
        if (!MgLiveObjectJson(data)) return MgLiveResult(id, "invalid or oversized object GFF JSON");
        if (!MgLivePosition(j) || !MgLiveNumber(JsonObjectGet(j, "f"))) return MgLiveResult(id, "finite xyz/f required");
        location dest = Location(area, Vector(JsonGetFloat(JsonObjectGet(j, "x")), JsonGetFloat(JsonObjectGet(j, "y")), JsonGetFloat(JsonObjectGet(j, "z"))), JsonGetFloat(JsonObjectGet(j, "f")));
        object made = JsonToObject(data, dest, OBJECT_INVALID, FALSE);
        if (!GetIsObjectValid(made)) return MgLiveResult(id, "JsonToObject failed");
        SetLocalInt(made, "MGLIVE_OWN", TRUE);
        return MgLiveResult(id, "", made);
    }

    int inventoryOrder = op=="give_item" || op=="equip_item" || op=="unequip_item" || op=="drop_item" || op=="use_item" || op=="transfer_item";
    int economyOp = op == "take_gold" || op == "set_xp" || op == "take_xp" || op == "remove_level";
    int scriptOp = op == "run_script" || op == "run_chunk";
    object target = area;
    if (!scriptOp || JsonGetType(JsonObjectGet(j, "target")) != JSON_TYPE_NULL)
    {
        string targetId = JsonGetString(JsonObjectGet(j, "target"));
        if (!MgLiveObjectId(targetId)) return MgLiveResult(id, "invalid target id");
        target = MgLiveFindObject(targetId);
        if (!GetIsObjectValid(target)) return MgLiveResult(id, "target no longer exists", OBJECT_INVALID, "not_found");
        if (op != "unlimbo" && GetArea(target) != area) return MgLiveResult(id, "target is in another area", OBJECT_INVALID, "other_area");
        if ((inventoryOrder || economyOp || reputationOp || op=="play_creature_animation") && MgLiveProtected(target) && !allowPlayers)
            return MgLiveResult(id, "player operations disabled", OBJECT_INVALID, "disabled");
        if ((reputationOp || inventoryOrder || op=="play_creature_animation" || economyOp || scriptOp || op == "patrol" || op == "npc_move" || op == "npc_attack" || op == "npc_say" || op == "set_faction" ||
             op == "freeze" || op == "unfreeze" || op == "kill" || op == "heal" || op == "force_rest" || op == "limbo" || op == "unlimbo" || (playerOp && !playerExtra && op != "pc_raise")) && GetIsDead(target))
            return MgLiveResult(id, "target is dead", OBJECT_INVALID, "dead");
        if (!scriptOp && !playerOp && MgLiveProtected(target) &&
            !(allowPlayers && (reputationOp || inventoryOrder || op=="play_creature_animation" || economyOp || !MgLiveDirectPlayer(target) || op == "npc_move" || op == "npc_attack" || op == "set_appearance" || op == "replace_object_animation" || op == "apply_vfx" || op == "clear_vfx" || op == "freeze" || op == "unfreeze" || op == "kill" || op == "heal" || op == "set_invulnerable" || op == "set_immortal" || op == "set_ai" || op == "force_rest" || op == "view_inventory" || op == "take_item")))
            return MgLiveResult(id, "target is a player, DM or their associate", OBJECT_INVALID, "protected");
    }
    if (scriptOp)
    {
        // An omitted target means the area. Script commands may deliberately affect a PC/DM.
        if (op == "run_script")
        {
            string script = JsonGetString(JsonObjectGet(j, "script"));
            if (!MgLiveResRef(script) || FindSubString(script, "-") >= 0)
                return MgLiveResult(id, "invalid script resref");
            if (ResManGetAliasFor(script, RESTYPE_NCS) == "") return MgLiveResult(id, "unknown script", OBJECT_INVALID, "unknown_script");
            WriteTimestampedLogEntry("MGLIVE: script " + id + " on " + ObjectToString(target) + ": " + script);
            MgLiveCancelHold(target);
            ExecuteScript(script, target);
            return MgLiveResult(id);
        }
        if (GetLocalString(GetModule(), "MGLIVE_ALLOW_CHUNK") != "1")
            return MgLiveResult(id, "chunks are disabled on this server", OBJECT_INVALID, "disabled");
        string code = JsonGetString(JsonObjectGet(j, "code"));
        int n = GetStringLength(code);
        if (n < 1 || n > 8000) return MgLiveResult(id, "code must be 1-8000 nonblank characters");
        int i;
        int nonblank = FALSE;
        for (i = 0; i < n && !nonblank; i++)
            nonblank = FindSubString(" \t\r\n", GetSubString(code, i, 1)) < 0;
        if (!nonblank) return MgLiveResult(id, "code must be 1-8000 nonblank characters");
        json wrapping = JsonObjectGet(j, "wrap");
        int wrap = TRUE;
        if (JsonGetType(wrapping) != JSON_TYPE_NULL)
        {
            if (JsonGetType(wrapping) != JSON_TYPE_BOOL) return MgLiveResult(id, "wrap must be boolean");
            wrap = JsonGetInt(wrapping);
        }
        WriteTimestampedLogEntry("MGLIVE: chunk " + id + " on " + ObjectToString(target) + ": " + GetStringLeft(code, 200));
        MgLiveCancelHold(target);
        json returning = JsonObjectGet(j, "return");
        if (JsonGetType(returning) != JSON_TYPE_NULL && JsonGetType(returning) != JSON_TYPE_BOOL)
            return MgLiveResult(id, "return must be boolean");
        int wantReturn = JsonGetInt(returning);
        object m = GetModule();
        DeleteLocalString(m, "MGLIVE_RETURN");
        DeleteLocalString(m, "MGLIVE_RETURN_ERROR");
        SetLocalInt(m, "MGLIVE_RETURN_ACTIVE", wantReturn);
        if (wantReturn)
        {
            if (wrap) code = "void main() {\n" + code + "\n}";
            code = "#include \"mglive_ret\"\n" + code;
            wrap = FALSE;
        }
        string error = GetStringLeft(ExecuteScriptChunk(code, target, wrap), 16384);
        string returned = GetLocalString(m, "MGLIVE_RETURN");
        string invalid = GetLocalString(m, "MGLIVE_RETURN_ERROR");
        DeleteLocalString(m, "MGLIVE_RETURN");
        DeleteLocalString(m, "MGLIVE_RETURN_ERROR");
        DeleteLocalInt(m, "MGLIVE_RETURN_ACTIVE");
        if (error != "") return MgLiveResult(id, error, OBJECT_INVALID, "compile_error");
        if (invalid != "") return MgLiveResult(id, invalid);
        json result = MgLiveResult(id);
        if (wantReturn)
        {
            json value = JsonNull();
            if (returned != "") value = JsonParse(returned);
            result = JsonObjectSet(result, "value", value);
        }
        return result;
    }
    int t = MgLiveType(target);
    if(op=="destroy" && t==OBJECT_TYPE_ENCOUNTER) return MgLiveResult(id,"native encounter destruction did not remove the object; no safe setter available",OBJECT_INVALID,"disabled");
    if(op=="destroy" && (t==OBJECT_TYPE_WAYPOINT || t==MGLIVE_TYPE_SOUND || t==OBJECT_TYPE_ENCOUNTER || t==OBJECT_TYPE_STORE || t==OBJECT_TYPE_TRIGGER)) { DestroyObject(target);return MgLiveResult(id); }
    if (t != OBJECT_TYPE_CREATURE && t != OBJECT_TYPE_PLACEABLE && t != OBJECT_TYPE_DOOR && t != OBJECT_TYPE_ITEM)
        return MgLiveResult(id, "unsupported target type");
    if (playerExtra && (!GetIsPC(target) || GetIsDM(target) || GetIsDMPossessed(target))) return MgLiveResult(id, "target must be PC, not NPC or DM", OBJECT_INVALID, "protected");
    if (op == "replace_object_animation") return MgLiveReplace(j, target, id);
    if (op == "pc_message" || op == "pc_boot")
    {
        string field = "text"; if (op == "pc_boot") field = "reason";
        json raw = JsonObjectGet(j,field); string text = JsonGetString(raw);
        if ((JsonGetType(raw) != JSON_TYPE_STRING && !(op == "pc_boot" && JsonGetType(raw) == JSON_TYPE_NULL)) || (!MgLiveText(text) && !(op == "pc_boot" && text == ""))) return MgLiveResult(id,"text/reason must be 1-200 nonblank characters; boot reason may be empty");
        if (op == "pc_boot")
        {
            AssignCommand(GetModule(),DelayCommand(0.5,BootPC(target,text)));
            return MgLiveResult(id);
        }
        string mode = JsonGetString(JsonObjectGet(j,"mode")); int token = 0;
        if (mode == "dm") return JsonObjectSet(JsonObjectSet(MgLiveResult(id),"text_kind",JsonString("dm_reply")),"value",MgLiveDmReply(target,text));
        if (mode == "chat") SendMessageToPC(target,text);
        else if (mode == "floating") FloatingTextStringOnCreature(text,target,FALSE,FALSE);
        else if (mode == "nui") token = MgLiveShowMessage(target,text);
        else return MgLiveResult(id,"mode must be chat, dm, floating or nui");
        return JsonObjectSet(MgLiveResult(id),"value",JsonObjectSet(JsonObject(),"nui_token",JsonInt(token)));
    }
    if (op == "pc_journal")
    {
        json quest = JsonObjectGet(j,"quest"); json stage = JsonObjectGet(j,"state"); string tag = JsonGetString(quest);
        if (JsonGetType(quest) != JSON_TYPE_STRING || GetStringLength(tag)<1 || GetStringLength(tag)>64 || !MgLiveInt(stage,0,2147483647)) return MgLiveResult(id,"quest 1-64 ASCII tag characters, state 0..2147483647 required");
        int i; for(i=0;i<GetStringLength(tag);i++) if(FindSubString("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-",GetSubString(tag,i,1))<0) return MgLiveResult(id,"invalid quest tag");
        AddJournalQuestEntry(tag,JsonGetInt(stage),target,FALSE,FALSE,TRUE);
        return MgLiveResult(id);
    }
    if (op == "fade")
    {
        json sec = JsonObjectGet(j,"seconds"); string direction = JsonGetString(JsonObjectGet(j,"direction"));
        if (!MgLiveNumber(sec) || JsonGetFloat(sec)<=0.0 || JsonGetFloat(sec)>60.0) return MgLiveResult(id,"seconds must be >0 and <=60");
        if (direction=="to_black") FadeToBlack(target,JsonGetFloat(sec));
        else if (direction=="from_black") FadeFromBlack(target,JsonGetFloat(sec));
        else return MgLiveResult(id,"direction must be to_black or from_black");
        return MgLiveResult(id);
    }
    if (op == "play_sound")
    {
        string sound = JsonGetString(JsonObjectGet(j,"resref"));
        if (!MgLiveResRef(sound)) return MgLiveResult(id,"invalid sound resref");
        if (ResManGetAliasFor(sound,RESTYPE_WAV)=="") return MgLiveResult(id,"sound WAV not found",OBJECT_INVALID,"not_found");
        AssignCommand(target,PlaySound(sound));
        return MgLiveResult(id);
    }
    if (inventoryOrder && op!="give_item") return MgLiveInventoryOrder(j,target,allowPlayers);
    if (op=="play_creature_animation") return MgLiveCreatureAnimation(j,target);
    if (economyOp) return MgLiveEconomy(j, target);
    if (op == "capture_object")
    {
        if (MgLiveDirectPlayer(target)) return MgLiveResult(id, "PC/DM snapshots are not an NPC library", OBJECT_INVALID, "protected");
        json snapshot = ObjectToJson(target, FALSE);
        if (!MgLiveObjectJson(snapshot)) return MgLiveResult(id, "object JSON invalid or exceeds 256 KiB at " + GetLocalString(GetModule(), "MGLIVE_JSON_FIELD"));
        return JsonObjectSet(MgLiveResult(id), "object_json", snapshot);
    }
    if (op == "disable_trap" || op == "spawn_trap_on_object")
    {
        if (t != OBJECT_TYPE_DOOR && t != OBJECT_TYPE_PLACEABLE) return MgLiveResult(id, "trap target must be door or placeable");
        if (op == "disable_trap") SetTrapDisabled(target);
        else
        {
            json row = JsonObjectGet(j, "trap");
            if (!MgLiveInt(row, 0, 65535)) return MgLiveResult(id, "trap must be a traps.2da row 0-65535");
            string label = Get2DAString("traps", "Label", JsonGetInt(row));
            if (label == "" || label == "****") return MgLiveResult(id, "trap row not found", OBJECT_INVALID, "not_found");
            if (GetIsTrapped(target) && GetTrapActive(target)) return MgLiveResult(id, "target already has an active trap");
            CreateTrapOnObject(JsonGetInt(row), target);
        }
        return MgLiveResult(id);
    }
    if (op == "view_inventory" || op == "take_item" || op == "pc_take_item")
    {
        if (t != OBJECT_TYPE_CREATURE && t != OBJECT_TYPE_PLACEABLE) return MgLiveResult(id, "inventory holder must be creature or placeable");
        if (op == "take_item" || op == "pc_take_item")
        {
            json requestedItem = JsonObjectGet(j, "item");
            json requestedResref = JsonObjectGet(j, "resref");
            int byId = JsonGetType(requestedItem) != JSON_TYPE_NULL;
            int byResref = JsonGetType(requestedResref) != JSON_TYPE_NULL;
            if (byId == byResref) return MgLiveResult(id, "supply item or resref exclusively");
            object item;
            if (byId)
            {
                if (JsonGetType(requestedItem) != JSON_TYPE_STRING || !MgLiveObjectId(JsonGetString(requestedItem))) return MgLiveResult(id, "invalid item id");
                item = MgLiveFindObject(JsonGetString(requestedItem));
            }
            else
            {
                string resref = JsonGetString(requestedResref);
                if (!MgLiveResRef(resref)) return MgLiveResult(id, "invalid item resref");
                item = GetFirstItemInInventory(target);
                while (GetIsObjectValid(item) && GetResRef(item) != resref) item = GetNextItemInInventory(target);
                if (!GetIsObjectValid(item) && t == OBJECT_TYPE_CREATURE)
                {
                    int slot;
                    for (slot = 0; slot < NUM_INVENTORY_SLOTS && !GetIsObjectValid(item); slot++)
                    {
                        object equipped = GetItemInSlot(slot, target);
                        if (GetIsObjectValid(equipped) && GetResRef(equipped) == resref) item = equipped;
                    }
                }
            }
            if (!GetIsObjectValid(item) || GetObjectType(item) != OBJECT_TYPE_ITEM || GetItemPossessor(item) != target)
                return MgLiveResult(id, "item not in selected inventory", OBJECT_INVALID, "not_found");
            if (GetPlotFlag(item)) return MgLiveResult(id, "plot item", OBJECT_INVALID, "protected");
            json taken = JsonString(ObjectToString(item));
            DestroyObject(item);
            return JsonObjectSet(MgLiveResult(id), "value", taken);
        }
        json rows = JsonArray();
        json selected = JsonArray();
        int slotIndex;
        if (t == OBJECT_TYPE_CREATURE)
            for (slotIndex = 0; slotIndex < NUM_INVENTORY_SLOTS; slotIndex++)
            {
                object equipped = GetItemInSlot(slotIndex, target);
                if (GetIsObjectValid(equipped) && !MgLiveContains(selected, ObjectToString(equipped)))
                    selected = JsonArrayInsert(selected, JsonString(ObjectToString(equipped)));
            }
        object item = GetFirstItemInInventory(target);
        while (GetIsObjectValid(item) && JsonGetLength(selected) <= 128)
        {
            if (!MgLiveContains(selected, ObjectToString(item))) selected = JsonArrayInsert(selected, JsonString(ObjectToString(item)));
            item = GetNextItemInInventory(target);
        }
        int truncated = GetIsObjectValid(item);
        int selectedIndex;
        for (selectedIndex = 0; selectedIndex < JsonGetLength(selected); selectedIndex++)
        {
            item = StringToObject(JsonGetString(JsonArrayGet(selected, selectedIndex)));
            json row = JsonObjectSet(JsonObject(), "id", JsonString(ObjectToString(item)));
            row = JsonObjectSet(row, "resref", JsonString(GetResRef(item)));
            row = MgLiveTextField(row, "tag", GetTag(item));
            row = MgLiveTextField(row, "name", GetName(item));
            row = JsonObjectSet(row, "stack", JsonInt(GetItemStackSize(item)));
            row = JsonObjectSet(row, "look", MgLiveItemLook(item));
            row = JsonObjectSet(row, "equip_slots", JsonInt(MgLiveEquipSlots(item)));
            row = JsonObjectSet(row, "uses", MgLiveItemUses(item));
            row = JsonObjectSet(row, "plot", JsonBool(GetPlotFlag(item)));
            row = JsonObjectSet(row, "droppable", JsonBool(GetDroppableFlag(item)));
            int slot = -1; int i;
            if (t == OBJECT_TYPE_CREATURE)
                for (i = 0; i < NUM_INVENTORY_SLOTS; i++)
                    if (GetItemInSlot(i, target) == item) slot = i;
            row = JsonObjectSet(row, "slot", JsonInt(slot));
            json candidate = JsonArrayInsert(rows, row);
            if (JsonGetLength(candidate) > 128 || GetStringLength(JsonDump(candidate)) > 15000) { truncated = TRUE; break; }
            rows = candidate;
        }
        json result = JsonObjectSet(JsonObject(), "items", rows);
        result = JsonObjectSet(result, "truncated", JsonBool(truncated));
        return JsonObjectSet(JsonObjectSet(MgLiveResult(id), "text_kind", JsonString("inventory")), "value", result);
    }
    if (op == "patrol" || op == "stop_patrol")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target must be a creature");
        if (op == "stop_patrol") { if (GetLocalInt(target, "MGLIVE_PATROL_ACTIVE")) MgLiveCancelHold(target); return MgLiveResult(id); }
        json points = JsonObjectGet(j, "points");
        json loop = JsonObjectGet(j, "loop"); json run = JsonObjectGet(j, "run");
        if (JsonGetType(points) != JSON_TYPE_ARRAY || JsonGetLength(points) < 1 || JsonGetLength(points) > 16)
            return MgLiveResult(id, "points must contain 1-16 xyz objects");
        if ((JsonGetType(loop) != JSON_TYPE_NULL && JsonGetType(loop) != JSON_TYPE_BOOL) ||
            (JsonGetType(run) != JSON_TYPE_NULL && JsonGetType(run) != JSON_TYPE_BOOL)) return MgLiveResult(id, "loop/run must be booleans");
        int n;
        for (n = 0; n < JsonGetLength(points); n++)
            if (!MgLivePosition(JsonArrayGet(points, n))) return MgLiveResult(id, "finite point xyz required");
        int generation = MgLiveNewOrder(target);
        SetLocalInt(target, "MGLIVE_PATROL_ACTIVE", generation);
        SetLocalString(target,"MGLIVE_PATROL_COMMAND",id);
        SetLocalString(target,"MGLIVE_PATROL_BY",GetLocalString(GetModule(),"MGLIVE_COMMAND_BY"));
        SetLocalJson(target, "MGLIVE_PATROL_POINTS", points);
        SetLocalInt(target, "MGLIVE_PATROL_INDEX", 0);
        SetLocalInt(target, "MGLIVE_PATROL_LAP", 0);
        SetLocalInt(target, "MGLIVE_PATROL_LOOP", JsonGetInt(loop));
        SetLocalInt(target, "MGLIVE_PATROL_RUN", JsonGetInt(run));
        SetLocalObject(target, "MGLIVE_PATROL_AREA", area);
        SetLocalInt(target, "MGLIVE_HELD", generation);
        MgLivePatrolIssue(target, generation);
        return MgLiveResult(id);
    }
    if (op == "set_static")
    {
        if (t != OBJECT_TYPE_PLACEABLE) return MgLiveResult(id, "target must be a placeable");
        json value = JsonObjectGet(j, "static");
        if (JsonGetType(value) != JSON_TYPE_BOOL) return MgLiveResult(id, "static must be boolean");
        NWNX_Object_SetPlaceableIsStatic(target, JsonGetInt(value));
        json readback = JsonObjectSet(JsonObject(), "static", JsonBool(NWNX_Object_GetPlaceableIsStatic(target)));
        return JsonObjectSet(MgLiveResult(id), "value", readback);
    }
    if (op == "set_lootable") return MgLiveLoot(j, target, id);
    if (op == "set_appearance")
    {
        if (t != OBJECT_TYPE_CREATURE && t != OBJECT_TYPE_PLACEABLE) return MgLiveResult(id, "target must be a creature or placeable");
        json row = JsonObjectGet(j, "row");
        if (!MgLiveInt(row, 0, 65535)) return MgLiveResult(id, "row must be integer 0-65535");
        string table = "appearance";
        if (t == OBJECT_TYPE_PLACEABLE) table = "placeables";
        string label = Get2DAString(table, "LABEL", JsonGetInt(row));
        if (label == "" || label == "****") return MgLiveResult(id, "appearance row not found", OBJECT_INVALID, "not_found");
        int actual;
        if (t == OBJECT_TYPE_PLACEABLE)
        {
            MgLiveOriginalAppearance(target, NWNX_Object_GetAppearance(target));
            NWNX_Object_SetAppearance(target, JsonGetInt(row));
            actual = NWNX_Object_GetAppearance(target);
        }
        else
        {
            MgLiveOriginalAppearance(target, GetAppearanceType(target));
            SetCreatureAppearanceType(target, JsonGetInt(row));
            actual = GetAppearanceType(target);
        }
        if (actual != JsonGetInt(row)) return MgLiveResult(id, "appearance was not applied");
        return JsonObjectSet(MgLiveResult(id), "value", JsonObjectSet(JsonObject(), "app", JsonInt(actual)));
    }
    if (op == "apply_vfx") return MgLiveApplyVfx(j, area, target);
    if (op == "clear_vfx")
    {
        json row = JsonObjectGet(j, "row");
        int filter = JsonGetType(row) != JSON_TYPE_NULL;
        if (filter && !MgLiveInt(row, 0, 65535)) return MgLiveResult(id, "row must be integer 0-65535");
        int count = 0;
        effect current = GetFirstEffect(target);
        while (GetIsEffectValid(current))
        {
            if (GetEffectTag(current) == "MGLIVE_VFX" && GetEffectType(current, TRUE) == EFFECT_TYPE_VISUALEFFECT &&
                (!filter || GetEffectInteger(current, 0) == JsonGetInt(row)))
            {
                RemoveEffect(target, current);
                count++;
            }
            current = GetNextEffect(target);
        }
        return JsonObjectSet(MgLiveResult(id), "value", JsonObjectSet(JsonObject(), "removed", JsonInt(count)));
    }
    if (op == "kill" || op == "heal" || op == "set_invulnerable" || op == "set_immortal" ||
        op == "set_ai" || op == "force_rest" || op == "limbo" || op == "unlimbo")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        if (op == "kill")
        {
            int plot = GetPlotFlag(target);
            int immortal = GetImmortal(target);
            MgLiveCancelHold(target);
            SetPlotFlag(target, FALSE);
            SetImmortal(target, FALSE);
            ApplyEffectToObject(DURATION_TYPE_INSTANT, SupernaturalEffect(EffectDeath(FALSE, TRUE)), target);
            int died = GetIsDead(target);
            if(died) MgLiveObserveDeath(target);
            // A forced death must not permanently edit the blueprint's protections.
            SetPlotFlag(target, plot);
            SetImmortal(target, immortal);
            json result = MgLiveResult(id);
            if (!died) result = MgLiveResult(id, "death effect did not kill target");
            json value = JsonObjectSet(JsonObject(), "dead", JsonBool(died));
            json overridden = JsonObjectSet(JsonObject(), "plot", JsonBool(plot));
            overridden = JsonObjectSet(overridden, "immortal", JsonBool(immortal));
            value = JsonObjectSet(value, "overridden", overridden);
            return JsonObjectSet(result, "value", value);
        }
        else if (op == "heal")
        {
            json amountValue = JsonObjectGet(j, "amount");
            int amount = GetMaxHitPoints(target);
            if (JsonGetType(amountValue) != JSON_TYPE_NULL)
            {
                if (!MgLiveInt(amountValue, 1, 100000))
                    return MgLiveResult(id, "amount must be 1-100000");
                amount = JsonGetInt(amountValue);
            }
            ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectHeal(amount), target);
        }
        else if (op == "set_invulnerable" || op == "set_immortal")
        {
            json enabled = JsonObjectGet(j, "enabled");
            if (JsonGetType(enabled) != JSON_TYPE_BOOL) return MgLiveResult(id, "enabled must be boolean");
            if (op == "set_invulnerable") SetPlotFlag(target, JsonGetInt(enabled));
            else SetImmortal(target, JsonGetInt(enabled));
        }
        else if (op == "set_ai")
        {
            string level = JsonGetString(JsonObjectGet(j, "level"));
            int ai = -2;
            if (level == "default") ai = AI_LEVEL_DEFAULT;
            else if (level == "very_low") ai = AI_LEVEL_VERY_LOW;
            else if (level == "low") ai = AI_LEVEL_LOW;
            else if (level == "normal") ai = AI_LEVEL_NORMAL;
            else if (level == "high") ai = AI_LEVEL_HIGH;
            else if (level == "very_high") ai = AI_LEVEL_VERY_HIGH;
            if (ai == -2) return MgLiveResult(id, "invalid AI level");
            SetAILevel(target, ai);
            json actual = JsonObjectSet(JsonObject(), "requested", JsonInt(ai));
            actual = JsonObjectSet(actual, "level", JsonInt(GetAILevel(target)));
            return JsonObjectSet(MgLiveResult(id), "value", actual);
        }
        else if (op == "force_rest")
        {
            MgLiveCancelHold(target);
            ForceRest(target);
        }
        else if (op == "limbo")
        {
            if (MgLiveDirectPlayer(target)) return MgLiveResult(id, "native limbo does not support players or DMs", OBJECT_INVALID, "protected");
            MgLiveCancelHold(target);
            AssignCommand(target, ClearAllActions(TRUE));
            NWNX_Creature_JumpToLimbo(target);
        }
        else
        {
            if (MgLiveDirectPlayer(target)) return MgLiveResult(id, "native limbo does not support players or DMs", OBJECT_INVALID, "protected");
            if (GetIsObjectValid(GetArea(target))) return MgLiveResult(id, "target is not in limbo");
            if (!MgLivePosition(j)) return MgLiveResult(id, "invalid coordinates");
            vector p = Vector(JsonGetFloat(JsonObjectGet(j, "x")), JsonGetFloat(JsonObjectGet(j, "y")), JsonGetFloat(JsonObjectGet(j, "z")));
            AssignCommand(target, JumpToLocation(Location(area, p, GetFacing(target))));
        }
        return MgLiveResult(id);
    }
    int hold = FALSE;
    int timeout = 30;
    if (op == "npc_move" || op == "npc_attack")
    {
        json held = JsonObjectGet(j, "hold");
        json duration = JsonObjectGet(j, "timeout");
        if (JsonGetType(held) != JSON_TYPE_NULL && JsonGetType(held) != JSON_TYPE_BOOL)
            return MgLiveResult(id, "hold must be boolean");
        hold = JsonGetInt(held);
        if (JsonGetType(duration) != JSON_TYPE_NULL)
        {
            if (JsonGetType(duration) != JSON_TYPE_INTEGER) return MgLiveResult(id, "timeout must be an integer");
            timeout = JsonGetInt(duration);
            if (timeout < 1 || timeout > 120) return MgLiveResult(id, "timeout must be 1-120 seconds");
        }
    }
    if (op == "give_level" || op == "give_alignment" || op == "set_stat" || op == "get_faction_reputation" || op == "set_faction_reputation")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target must be a creature");
        json value = JsonObject();
        if (op == "give_level")
        {
            int level = GetHitDice(target);
            if (level < 1 || level >= 40) return MgLiveResult(id, "supported levels are 1-39");
            string threshold = Get2DAString("exptable", "XP", level);
            int xp = StringToInt(threshold);
            if (threshold == "" || threshold == "****" || xp <= 0) return MgLiveResult(id, "next XP threshold unavailable", OBJECT_INVALID, "not_found");
            if (GetXP(target) >= xp) return MgLiveResult(id, "already eligible for next level");
            int grant = xp - GetXP(target);
            GiveXPToCreature(target, grant);
            value = JsonObjectSet(value, "xp", JsonInt(GetXP(target)));
            value = JsonObjectSet(value, "eligible_level", JsonInt(level+1));
            value = JsonObjectSet(value, "granted", JsonInt(grant));
        }
        else if (op == "give_alignment")
        {
            json lc = JsonObjectGet(j, "law_chaos"); json ge = JsonObjectGet(j, "good_evil");
            if (!MgLiveInt(lc, -100, 100) || !MgLiveInt(ge, -100, 100)) return MgLiveResult(id, "alignment deltas must be integers -100..100");
            // AdjustAlignment crosses alignment categories with extra native
            // shifts (50 minus 20 became 15). This wire requests exact deltas.
            int law = GetLawChaosValue(target) + JsonGetInt(lc);
            int good = GetGoodEvilValue(target) + JsonGetInt(ge);
            if (law < 0) law = 0; if (law > 100) law = 100;
            if (good < 0) good = 0; if (good > 100) good = 100;
            NWNX_Creature_SetAlignmentLawChaos(target, law);
            NWNX_Creature_SetAlignmentGoodEvil(target, good);
            value = JsonObjectSet(value, "law_chaos", JsonInt(GetLawChaosValue(target)));
            value = JsonObjectSet(value, "good_evil", JsonInt(GetGoodEvilValue(target)));
        }
        else if (op == "set_stat")
        {
            string ability = JsonGetString(JsonObjectGet(j, "ability"));
            int stat = -1;
            if (ability == "strength") stat = ABILITY_STRENGTH;
            else if (ability == "dexterity") stat = ABILITY_DEXTERITY;
            else if (ability == "constitution") stat = ABILITY_CONSTITUTION;
            else if (ability == "intelligence") stat = ABILITY_INTELLIGENCE;
            else if (ability == "wisdom") stat = ABILITY_WISDOM;
            else if (ability == "charisma") stat = ABILITY_CHARISMA;
            json raw = JsonObjectGet(j, "value");
            if (stat < 0 || !MgLiveInt(raw, 0, 255)) return MgLiveResult(id, "ability and integer value 0-255 required");
            NWNX_Creature_SetRawAbilityScore(target, stat, JsonGetInt(raw));
            value = JsonInt(NWNX_Creature_GetRawAbilityScore(target, stat));
        }
        else
        {
            int faction = MgLiveFaction(JsonObjectGet(j, "faction"));
            if (faction < 0) return MgLiveResult(id, "invalid faction");
            if (faction == 0) return MgLiveResult(id, "PC source faction cannot be adjusted", OBJECT_INVALID, "disabled");
            int before;
            object representative = OBJECT_INVALID;
            if (faction <= 4) before = GetStandardFactionReputation(faction-1, target);
            else
            {
                // Custom factions require an extant representative. Bound scans.
                object searchArea = GetFirstArea(); int areas; int examined;
                while (GetIsObjectValid(searchArea) && areas < 256 && examined < 4096 && !GetIsObjectValid(representative))
                {
                    object member = GetFirstObjectInArea(searchArea);
                    while (GetIsObjectValid(member) && examined < 4096)
                    {
                        examined++;
                        if (GetObjectType(member) == OBJECT_TYPE_CREATURE && NWNX_Creature_GetFaction(member) == faction && !GetPlotFlag(member))
                        { representative = member; break; }
                        member = GetNextObjectInArea(searchArea);
                    }
                    areas++; searchArea = GetNextArea();
                }
                if (!GetIsObjectValid(representative)) return MgLiveResult(id, "no editable custom faction representative found", OBJECT_INVALID, "not_found");
                before = GetFactionAverageReputation(representative, target);
            }
            if (before < 0) return MgLiveResult(id, "faction reputation unavailable", OBJECT_INVALID, "not_found");
            int after = before;
            if (op == "set_faction_reputation")
            {
                json requestedRep = JsonObjectGet(j, "reputation");
                if (!MgLiveInt(requestedRep, 0, 100)) return MgLiveResult(id, "reputation must be integer 0-100");
                if (faction <= 4)
                {
                    SetStandardFactionReputation(faction-1, JsonGetInt(requestedRep), target);
                    after = GetStandardFactionReputation(faction-1, target);
                }
                else
                {
                    int requested = JsonGetInt(requestedRep);
                    int reverse = GetReputation(target, representative);
                    AdjustReputation(target, representative, requested-before);
                    after = GetFactionAverageReputation(representative, target);
                    // EE's unpatched NPC AdjustReputation reads the reverse
                    // direction as its base. Fixed engines succeed above. On
                    // mismatch retry once against that native reverse value.
                    if (after != requested && !MgLiveDirectPlayer(target) && reverse >= 0)
                    {
                        AdjustReputation(target, representative, requested-reverse);
                        after = GetFactionAverageReputation(representative, target);
                        value = JsonObjectSet(value,"native_reverse_base_workaround",JsonBool(TRUE));
                    }
                }
            }
            value = JsonObjectSet(value, "faction", JsonInt(faction));
            value = JsonObjectSet(value, "reputation", JsonInt(after));
            value = JsonObjectSet(value, "before", JsonInt(before));
            if (op == "set_faction_reputation" && after != JsonGetInt(JsonObjectGet(j,"reputation")))
                return JsonObjectSet(MgLiveResult(id,"native reputation did not reach the requested value",OBJECT_INVALID,"disabled"),"value",value);
        }
        return JsonObjectSet(MgLiveResult(id), "value", value);
    }
    if (op == "pc_heal")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        json value = JsonObjectGet(j, "amount");
        int amount = GetMaxHitPoints(target);
        if (JsonGetType(value) != JSON_TYPE_NULL)
        {
            if (JsonGetType(value) != JSON_TYPE_INTEGER) return MgLiveResult(id, "amount must be an integer");
            amount = JsonGetInt(value);
            if (amount < 1 || amount > 100000) return MgLiveResult(id, "amount must be 1-100000");
        }
        ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectHeal(amount), target);
        WriteTimestampedLogEntry("MGLIVE: " + op + " " + id + " target=" + ObjectToString(target));
    }
    else if (op == "pc_raise")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        if (!GetIsDead(target)) return MgLiveResult(id, "target is already alive");
        MgLiveCancelHold(target);
        ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectResurrection(), target);
        ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectHeal(GetMaxHitPoints(target)), target);
        WriteTimestampedLogEntry("MGLIVE: " + op + " " + id + " target=" + ObjectToString(target));
    }
    else if (op == "pc_give_gold")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        json value = JsonObjectGet(j, "amount");
        if (JsonGetType(value) != JSON_TYPE_INTEGER) return MgLiveResult(id, "amount must be an integer");
        int amount = JsonGetInt(value);
        if (amount < 1 || amount > 1000000 || GetGold(target) > 2147483647 - amount)
            return MgLiveResult(id, "amount must be 1-1000000 without overflowing total gold");
        GiveGoldToCreature(target, amount);
        WriteTimestampedLogEntry("MGLIVE: " + op + " " + id + " target=" + ObjectToString(target) + " amount=" + IntToString(amount));
    }
    else if (op == "pc_give_xp")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        json value = JsonObjectGet(j, "amount");
        if (JsonGetType(value) != JSON_TYPE_INTEGER) return MgLiveResult(id, "amount must be an integer");
        int amount = JsonGetInt(value);
        if (amount < 1 || amount > 100000 || GetXP(target) > 2147483647 - amount)
            return MgLiveResult(id, "amount must be 1-100000 without overflowing total XP");
        GiveXPToCreature(target, amount);
        WriteTimestampedLogEntry("MGLIVE: " + op + " " + id + " target=" + ObjectToString(target) + " amount=" + IntToString(amount));
    }
    else if (op == "pc_give_item" || op == "give_item")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        string resref = JsonGetString(JsonObjectGet(j, "resref"));
        if (!MgLiveResRef(resref)) return MgLiveResult(id, "invalid item resref");
        if (ResManGetAliasFor(resref, RESTYPE_UTI) == "")
            return MgLiveResult(id, "item blueprint not found", OBJECT_INVALID, "not_found");
        object item = CreateItemOnObject(resref, target);
        if (!GetIsObjectValid(item)) return MgLiveResult(id, "item creation failed");
        WriteTimestampedLogEntry("MGLIVE: " + op + " " + id + " target=" + ObjectToString(target) + " resref=" + resref);
        return MgLiveResult(id, "", item);
    }
    else if (op == "set_facing")
    {
        if (t != OBJECT_TYPE_PLACEABLE && t != OBJECT_TYPE_DOOR) return MgLiveResult(id, "target must be a placeable or door");
        if (!MgLiveNumber(JsonObjectGet(j, "f"))) return MgLiveResult(id, "invalid facing");
        float facing = JsonGetFloat(JsonObjectGet(j, "f"));
        if (facing < 0.0 || facing >= 360.0) return MgLiveResult(id, "f must be at least 0 and less than 360");
        if (t == OBJECT_TYPE_DOOR) return MgLiveResult(id, "native door facing changes are unsupported; use visual rotation", OBJECT_INVALID, "disabled");
        SetFacing(facing, target);
    }
    else if (op == "set_visual_transform")
    {
        if (t != OBJECT_TYPE_PLACEABLE && t != OBJECT_TYPE_DOOR) return MgLiveResult(id, "target must be a placeable or door");
        json reset = JsonObjectGet(j, "reset");
        json scale = JsonObjectGet(j, "scale");
        json rotate = JsonObjectGet(j, "rotate");
        json translate = JsonObjectGet(j, "translate");
        int hasScale = JsonGetType(scale) != JSON_TYPE_NULL;
        int hasRotate = JsonGetType(rotate) != JSON_TYPE_NULL;
        int hasTranslate = JsonGetType(translate) != JSON_TYPE_NULL;
        if (JsonGetType(reset) != JSON_TYPE_NULL && JsonGetType(reset) != JSON_TYPE_BOOL)
            return MgLiveResult(id, "reset must be boolean");
        if (JsonGetInt(reset))
        {
            if (hasScale || hasRotate || hasTranslate) return MgLiveResult(id, "reset cannot include transform values");
            ClearObjectVisualTransform(target, OBJECT_VISUAL_TRANSFORM_DATA_SCOPE_BASE);
        }
        else
        {
            if (!hasScale && !hasRotate && !hasTranslate) return MgLiveResult(id, "transform values required");
            if (hasScale && (!MgLiveNumber(scale) || JsonGetFloat(scale) <= 0.0 || JsonGetFloat(scale) > 10.0))
                return MgLiveResult(id, "scale must be greater than 0 and at most 10");
            if (hasRotate && !MgLiveTransformVector(rotate, 360.0)) return MgLiveResult(id, "rotate requires finite xyz in -360..360 degrees");
            if (hasTranslate && !MgLiveTransformVector(translate, 1000.0)) return MgLiveResult(id, "translate requires finite xyz in -1000..1000 meters");
            if (hasScale) SetObjectVisualTransform(target, OBJECT_VISUAL_TRANSFORM_SCALE, JsonGetFloat(scale));
            int axis;
            for (axis = 0; axis < 3; axis++)
            {
                string key = GetSubString("xyz", axis, 1);
                if (hasRotate) SetObjectVisualTransform(target, OBJECT_VISUAL_TRANSFORM_ROTATE_X + axis, JsonGetFloat(JsonObjectGet(rotate, key)));
                if (hasTranslate) SetObjectVisualTransform(target, OBJECT_VISUAL_TRANSFORM_TRANSLATE_X + axis, JsonGetFloat(JsonObjectGet(translate, key)));
            }
        }
    }
    else if (op == "set_position")
    {
        if (t != OBJECT_TYPE_PLACEABLE && t != OBJECT_TYPE_DOOR) return MgLiveResult(id, "target must be a placeable or door");
        if (!MgLivePosition(j)) return MgLiveResult(id, "invalid coordinates");
        vector destination = Vector(JsonGetFloat(JsonObjectGet(j, "x")), JsonGetFloat(JsonObjectGet(j, "y")), JsonGetFloat(JsonObjectGet(j, "z")));
        NWNX_Object_SetPosition(target, destination, TRUE);
    }
    else if (op == "play_animation")
    {
        if (t != OBJECT_TYPE_PLACEABLE && t != OBJECT_TYPE_DOOR) return MgLiveResult(id, "target must be a placeable or door");
        string name = JsonGetString(JsonObjectGet(j, "name"));
        int animation = -1;
        if (name == "activate") animation = ANIMATION_PLACEABLE_ACTIVATE;
        if (name == "deactivate") animation = ANIMATION_PLACEABLE_DEACTIVATE;
        if (name == "open") animation = ANIMATION_PLACEABLE_OPEN;
        if (name == "close") animation = ANIMATION_PLACEABLE_CLOSE;
        if (animation < 0) return MgLiveResult(id, "unknown animation name");
        if (t == OBJECT_TYPE_DOOR)
        {
            if (name != "open" && name != "close") return MgLiveResult(id, "doors support open/close only");
            if (name == "open" && GetLocked(target)) return MgLiveResult(id, "door is locked");
            if (name == "open") AssignCommand(target, ActionOpenDoor(target));
            else AssignCommand(target, ActionCloseDoor(target));
        }
        else AssignCommand(target, PlayAnimation(animation));
    }
    else if (op == "set_useable")
    {
        if (t != OBJECT_TYPE_PLACEABLE && t != OBJECT_TYPE_DOOR) return MgLiveResult(id, "target must be a placeable or door");
        json value = JsonObjectGet(j, "useable");
        if (JsonGetType(value) != JSON_TYPE_BOOL) return MgLiveResult(id, "useable must be boolean");
        if (t == OBJECT_TYPE_PLACEABLE && NWNX_Object_GetPlaceableIsStatic(target))
            return MgLiveResult(id, "static placeables cannot change useability", OBJECT_INVALID, "disabled");
        SetUseableFlag(target, JsonGetInt(value));
    }
    else if (op == "set_name")
    {
        string name = JsonGetString(JsonObjectGet(j, "name"));
        if (!MgLiveText(name)) return MgLiveResult(id, "name must be 1-200 nonblank bytes");
        SetName(target, name);
    }
    else if (op == "freeze" || op == "unfreeze")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        float seconds = 0.0;
        if (op == "freeze")
        {
            if (!MgLiveNumber(JsonObjectGet(j, "seconds"))) return MgLiveResult(id, "invalid freeze duration");
            seconds = JsonGetFloat(JsonObjectGet(j, "seconds"));
            if (seconds <= 0.0 || seconds > 300.0) return MgLiveResult(id, "seconds must be greater than 0 and at most 300");
        }
        if (GetLocalInt(target, "MGLIVE_PATROL_ACTIVE")) MgLiveCancelHold(target);
        MgLiveUnfreeze(target);
        if (op == "freeze")
            ApplyEffectToObject(DURATION_TYPE_TEMPORARY,
                TagEffect(SupernaturalEffect(EffectCutsceneParalyze()), "MGLIVE_FREEZE"), target, seconds);
    }
    else if (op == "set_faction")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        int faction = MgLiveFaction(JsonObjectGet(j, "faction"));
        if (faction < 0) return MgLiveResult(id, "invalid faction name or id");
        // The plugin safely ignores nonexistent IDs. Confirm existence before
        // canceling a hold, so refusal does not alter the target's live order.
        NWNX_Creature_SetFaction(target, faction);
        if (NWNX_Creature_GetFaction(target) != faction)
            return MgLiveResult(id, "faction does not exist");
        MgLiveCancelHold(target);
        NWNX_Creature_SetFaction(target, faction);
        return JsonObjectSet(MgLiveResult(id), "faction", JsonInt(faction));
    }
    else if (op == "destroy")
    {
        MgLiveCancelHold(target);
        DestroyObject(target);
    }
    else if (op == "npc_move")
    {
        if (t != OBJECT_TYPE_CREATURE) return MgLiveResult(id, "target is not a creature");
        if (!MgLivePosition(j)) return MgLiveResult(id, "invalid coordinates");
        json run = JsonObjectGet(j, "run");
        if (JsonGetType(run) != JSON_TYPE_NULL && JsonGetType(run) != JSON_TYPE_BOOL)
            return MgLiveResult(id, "run must be boolean");
        vector p = Vector(JsonGetFloat(JsonObjectGet(j, "x")), JsonGetFloat(JsonObjectGet(j, "y")), JsonGetFloat(JsonObjectGet(j, "z")));
        int generation = MgLiveNewOrder(target);
        AssignCommand(target, MgLiveMove(Location(area, p, 0.0), JsonGetInt(run), hold, generation));
        if (hold)
        {
            DelayCommand(0.25, MgLiveWatchHold(target, OBJECT_INVALID, generation, FALSE));
            DelayCommand(IntToFloat(timeout), MgLiveReleaseHold(target, generation));
        }
    }
    else if (op == "npc_say")
    {
        if (t != OBJECT_TYPE_CREATURE && t != OBJECT_TYPE_PLACEABLE)
            return MgLiveResult(id, "target cannot speak");
        string line = JsonGetString(JsonObjectGet(j, "text"));
        if (!MgLiveText(line)) return MgLiveResult(id, "text must be 1-200 nonblank characters");
        int volume = TALKVOLUME_TALK;
        json requestedVolume = JsonObjectGet(j, "volume");
        if (JsonGetType(requestedVolume) != JSON_TYPE_NULL)
        {
            if (JsonGetType(requestedVolume) != JSON_TYPE_STRING) return MgLiveResult(id, "invalid speech volume");
            string label = JsonGetString(requestedVolume);
            if (label == "whisper") volume = TALKVOLUME_WHISPER;
            else if (label == "shout") volume = TALKVOLUME_SHOUT;
            else if (label != "talk") return MgLiveResult(id, "volume must be talk, whisper or shout");
        }
        MgLiveCancelHold(target);
        AssignCommand(target, MgLiveSpeak(line, volume, id, JsonGetString(JsonObjectGet(j,"by"))));
    }
    else if (op == "npc_attack")
    {
        string victimId = JsonGetString(JsonObjectGet(j, "victim"));
        if (!MgLiveObjectId(victimId)) return MgLiveResult(id, "invalid victim id");
        object victim = MgLiveFindObject(victimId);
        if (!GetIsObjectValid(victim)) return MgLiveResult(id, "victim no longer exists", OBJECT_INVALID, "not_found");
        if (GetArea(victim) != area) return MgLiveResult(id, "victim is in another area", OBJECT_INVALID, "other_area");
        if (GetIsDead(victim)) return MgLiveResult(id, "victim is dead", OBJECT_INVALID, "dead");
        if (t != OBJECT_TYPE_CREATURE || GetObjectType(victim) != OBJECT_TYPE_CREATURE || victim == target)
            return MgLiveResult(id, "attacker and victim must be different creatures");
        json requestedFaction = JsonObjectGet(j, "faction");
        int explicitFaction = JsonGetType(requestedFaction) != JSON_TYPE_NULL;
        int fightingFaction = -1;
        if (explicitFaction)
        {
            fightingFaction = MgLiveFaction(requestedFaction);
            if (fightingFaction <= 0 || fightingFaction == NWNX_Creature_GetFaction(victim))
                return MgLiveResult(id, "fighting faction must be non-PC and different from victim");
        }
        int generation = MgLiveNewOrder(target);
        int originalFaction = NWNX_Creature_GetFaction(target);
        int switched = explicitFaction || GetFactionEqual(target, victim);
        // DM force orders must never move a PC into an NPC/module faction.
        if (GetIsPC(target))
        {
            if (explicitFaction) return MgLiveResult(id,"fighting faction cannot change a PC");
            switched = FALSE;
        }
        if (switched)
        {
            if (originalFaction < 0) return MgLiveResult(id, "could not read attacker faction");
            if (!explicitFaction)
            {
                fightingFaction = 1;
                if (NWNX_Creature_GetFaction(victim) == 1) fightingFaction = 2;
            }
            NWNX_Creature_SetFaction(target, fightingFaction);
            if (NWNX_Creature_GetFaction(target) != fightingFaction)
                return MgLiveResult(id, "fighting faction does not exist", OBJECT_INVALID, "not_found");
            hold = TRUE;
            SetLocalInt(target, "MGLIVE_HELD", generation);
            SetLocalInt(target, "MGLIVE_FACTION", originalFaction + 1);
        }
        // Personal hostility is ignored within a faction; after switching the
        // factions differ, so this also supports neutral spare quest factions.
        SetIsTemporaryEnemy(victim, target, TRUE, 180.0);
        SetIsTemporaryEnemy(target, victim, TRUE, 180.0);
        AssignCommand(target, MgLiveAttack(victim, hold, generation));
        if (hold)
        {
            DelayCommand(0.25, MgLiveWatchHold(target, victim, generation, TRUE));
            DelayCommand(IntToFloat(timeout), MgLiveReleaseHold(target, generation));
        }
        if (switched)
            {
                json changed = JsonObjectSet(JsonObject(), "original", JsonInt(originalFaction));
                changed = JsonObjectSet(changed, "temporary", JsonInt(fightingFaction));
                return JsonObjectSet(MgLiveResult(id), "faction", changed);
            }
    }
    else if (op == "set_state")
    {
        if (t != OBJECT_TYPE_DOOR && t != OBJECT_TYPE_PLACEABLE)
            return MgLiveResult(id, "target is not a door or placeable");
        json open = JsonObjectGet(j, "open");
        json locked = JsonObjectGet(j, "lock");
        int hasOpen = JsonGetType(open) != JSON_TYPE_NULL;
        int hasLock = JsonGetType(locked) != JSON_TYPE_NULL;
        if ((!hasOpen && !hasLock) || (hasOpen && JsonGetType(open) != JSON_TYPE_BOOL) ||
            (hasLock && JsonGetType(locked) != JSON_TYPE_BOOL))
            return MgLiveResult(id, "state needs boolean open or lock");
        if (hasLock) SetLocked(target, JsonGetInt(locked));
        if (hasOpen)
        {
            if (t == OBJECT_TYPE_DOOR)
            {
                if (JsonGetInt(open)) AssignCommand(target, ActionOpenDoor(target));
                else AssignCommand(target, ActionCloseDoor(target));
            }
            else
            {
                int animation = ANIMATION_PLACEABLE_CLOSE;
                if (JsonGetInt(open)) animation = ANIMATION_PLACEABLE_OPEN;
                AssignCommand(target, PlayAnimation(animation));
            }
        }
    }
    else return MgLiveResult(id, "unknown operation");
    return MgLiveResult(id);
}
