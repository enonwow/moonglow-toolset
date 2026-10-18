//! The single operation catalogue used by discovery, dispatch and OpenAPI.
use crate::{
    now_ms,
    store::{LiveStore, Policy},
};
use serde_json::{Value, json};
use std::{
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[path = "assistant_ops.rs"]
mod assistant;
#[path = "atmosphere_ops.rs"]
mod atmosphere;

#[derive(Clone, Copy)]
enum Handler {
    Status,
    Area,
    Wait,
    Events,
    Find,
    Assistant(&'static str),
    Command(&'static str),
    Query(&'static str),
}

struct Operation {
    descriptor: Value,
    handler: Handler,
}

impl Operation {
    fn validate_input(
        &self,
        input: &Value,
        scope: &crate::access::Scope,
    ) -> Result<(), crate::store::CommandError> {
        if !scope.allows(self.descriptor["name"].as_str().unwrap_or_default()) {
            return Err(crate::store::CommandError::new(
                crate::store::ErrorCode::Forbidden,
                "Client scope does not allow this operation",
            ));
        }
        validate(&self.descriptor["input"], input).map_err(Into::into)
    }
}

// Only these three switches affect discovery. Descriptors are immutable and
// shared by validation, discovery and OpenAPI for the lifetime of the service.
fn table(policy: Policy) -> &'static [Operation] {
    static TABLES: [OnceLock<Vec<Operation>>; 8] = [const { OnceLock::new() }; 8];
    let index = usize::from(policy.scripts)
        | (usize::from(policy.chunks) << 1)
        | (usize::from(policy.players) << 2);
    TABLES[index].get_or_init(|| build_table(policy))
}

fn field(kind: &str, description: &str) -> Value {
    json!({"type":kind,"description":description})
}
fn text(description: &str, min: usize, max: usize) -> Value {
    json!({"type":"string","description":description,"minLength":min,"maxLength":max})
}
fn faction() -> Value {
    json!({"description":"Standard faction name, or native faction ID (the game checks existence).",
        "oneOf":[{"type":"string","enum":["hostile","commoner","merchant","defender"]},
            {"type":"integer","minimum":0,"maximum":65535}]})
}
fn vector(description: &str, limit: f64) -> Value {
    let axis =
        |name: &str| json!({"type":"number","description":name,"minimum":-limit,"maximum":limit});
    json!({"type":"object","description":description,"additionalProperties":false,
        "required":["x","y","z"],"properties":{"x":axis("X component."),"y":axis("Y component."),"z":axis("Z component.")}})
}
fn build_table(policy: Policy) -> Vec<Operation> {
    let area = || text("Area resref from live_status; not its tag.", 1, 16);
    let target = || text("Hexadecimal game object id from live_area_state in this area.", 1, 16);
    let point = || {
        [
            field("number", "X coordinate in metres."),
            field("number", "Y coordinate in metres."),
            field("number", "Z coordinate in metres."),
        ]
    };
    let mut ops = Vec::new();
    let mut add = |name: &str,
                   title: &str,
                   description: &str,
                   handler: Handler,
                   required: &[&str],
                   properties: Value| {
        let read_only = !matches!(handler, Handler::Command(_));
        ops.push(Operation { handler, descriptor:json!({"name":name,"group":"live","title":title,"description":description,
            "readOnly":read_only,"destructive":!read_only,"input":{"type":"object","additionalProperties":false,"required":required,"properties":properties}}) });
    };
    add(
        "live_status",
        "Live status",
        "Read game boot, report ages, areas, queued commands and results. redis.connected describes Moonglow's Redis link; game reports may still be stale.",
        Handler::Status,
        &[],
        json!({}),
    );
    add(
        "live_area_state",
        "Area state",
        "Read the current reported objects, their hexadecimal ids and report age for an area.",
        Handler::Area,
        &["area"],
        json!({"area":area()}),
    );
    add(
        "live_watch_areas",
        "Watch areas",
        "Start or stop reporting named areas. Areas occupied by players and explicit server extras still report. Does not require a previous area report. Use live_wait, then watched_areas in live_status.",
        Handler::Command("watch_areas"),
        &["areas", "on"],
        json!({"areas":{"type":"array","minItems":1,"maxItems":64,"items":area(),"description":"Area resrefs to watch or unwatch (case insensitive)."},"on":field("boolean","Whether these areas should be watched.")}),
    );
    add(
        "live_wait",
        "Wait for a command",
        "Wait for an asynchronous command's result. On timeout returns {id,pending:true}; call again or inspect live_status. Success is dispatch, not completion of movement/combat.",
        Handler::Wait,
        &["id"],
        json!({"id":text("Command id returned by a command operation.",1,64),"timeout_ms":{"type":"integer","description":"Maximum wait in milliseconds; shutdown interrupts the wait.","minimum":0,"maximum":30000,"default":5000}}),
    );
    add(
        "live_events",
        "Read live events",
        "Read what happened in the game, oldest first: chat (text, channel), attack, damage, death (killer), spell, vfx (projectiles), player_join/leave, server_log (line, script_error), command (the game's answers), and changes Moonglow saw between reports: entered, left, combat_start, combat_end, target, healed. Pass the returned last as since to read only newer events.",
        Handler::Events,
        &[],
        json!({"area":text("Only this area's events (resref); empty string: the server log and events without an area.",0,16),
            "since":{"type":"integer","description":"Only events numbered above this (the last value of a previous call).","minimum":0,"default":0},
            "kinds":{"type":"array","description":"Only these kinds; omitted: every kind.","items":text("An event kind, such as chat or death.",1,32)},
            "object":text("Only events with this object id as source or target.",1,16),
            "text":text("Only events whose name or content contains this text (any case).",1,200),
            "wait_ms":{"type":"integer","description":"Long-poll until a matching event arrives, timeout or service shutdown. Zero returns immediately.","minimum":0,"maximum":30000,"default":0},
            "from":{"type":"string","enum":["players"],"description":"Player or possessed chat speakers only; excludes silent AI shouts."},
            "to_dm":{"type":"boolean","description":"Filter chat by whether it was addressed to the DM."},
            "boot":text("Previous event response boot; a changed engine boot returns reset immediately.",1,64),
            "limit":{"type":"integer","description":"The newest this many that match.","minimum":1,"maximum":500,"default":100}}),
    );
    add(
        "live_find",
        "Find live objects",
        "Find reported objects in every area by part of their name, tag or blueprint resref: area, kind, id, position, player, dead and combat.",
        Handler::Find,
        &["text"],
        json!({"text":text("Part of a name, tag or resref (any case).",1,64),
            "limit":{"type":"integer","description":"At most this many objects.","minimum":1,"maximum":200,"default":50}}),
    );
    add(
        "live_view_inventory",
        "Inspect inventory",
        "Read at most 128 inventory/equipped items and 15KB; live_wait returns items and truncated. PCs require Players permission.",
        Handler::Query("view_inventory"),
        &["area", "target"],
        json!({"area":area(),"target":target()}),
    );
    let [x, y, z] = point();
    add(
        "live_teleport",
        "Teleport a live object",
        "Move a living creature, placeable, door or ground item. Optional area is the destination resref (omitted: target's current area). xy must be inside its native bounds. Creatures cancel holds/patrols and jump, including across areas; PCs require Players permission. Cross-area noncreatures are recreated before removing the original: live_wait returns obj and value.previous/recreated/area. Door facing changes also require recreation, even in the same area. Unreported destinations are validated by the game.",
        Handler::Command("teleport"),
        &["target", "x", "y", "z"],
        json!({"area":text("Optional destination area resref; default is target's current area",1,16),"target":text("Game object id from any reported area",1,16),"x":x,"y":y,"z":z,"f":{"type":"number","description":"Optional facing in degrees, omitted preserves it","minimum":0,"exclusiveMaximum":360}}),
    );
    add(
        "live_give_item",
        "Give an inventory item",
        "Create one item from a blueprint on an NPC, or a PC with Players permission. Returns obj. The legacy pc_give_item alias still requires Players for every target.",
        Handler::Command("give_item"),
        &["area", "target", "resref"],
        json!({"area":area(),"target":target(),"resref":text("Item blueprint resref",1,16)}),
    );
    for (name, title) in
        [("equip_item", "Equip an inventory item"), ("unequip_item", "Unequip an inventory item")]
    {
        add(
            &format!("live_{name}"),
            title,
            "Native equip/unequip with readback. Item must belong to the target. Unequip slot must match current equipment. NPCs permitted; PCs require Players permission. Native class/slot restrictions still apply.",
            Handler::Command(name),
            &["area", "target", "item", "slot"],
            json!({"area":area(),"target":target(),"item":text("Item object ID from view_inventory",1,16),"slot":{"type":"integer","description":"Native INVENTORY_SLOT 0-13; no hidden creature weapons/skin","minimum":0,"maximum":13}}),
        );
    }
    add(
        "live_use_item",
        "Use an inventory item",
        "Queue native ActionUseItemOnObject with charge consumption and native restrictions. property is an index from view_inventory uses; recipient defaults to self. Queued is not completed: verify charges/stack/effects. Both holder and recipient PCs require Players. An uncommandable holder is refused.",
        Handler::Command("use_item"),
        &["area", "target", "item"],
        json!({"area":area(),"target":target(),"item":target(),"property":{"type":"integer","description":"Cast ability property index (default 0).","minimum":0,"maximum":255},"recipient":target()}),
    );
    add(
        "live_transfer_item",
        "Give a held item to another creature",
        "Immediate native inventory copy with locals, then destroy original. Returns NEW obj/value.item and value.previous; outside references do not follow. Item must be droppable/non-plot. Same-area living recipient; both holder and recipient PCs require Players.",
        Handler::Command("transfer_item"),
        &["area", "target", "item", "recipient"],
        json!({"area":area(),"target":target(),"item":target(),"recipient":target()}),
    );
    add(
        "live_drop_item",
        "Drop an inventory item",
        "Drop immediately at the holder's feet using native CopyObject with local state, then destroy the held instance. Returns obj/value.item as the NEW ID and value.previous as the old ID; external references to the old ID do not follow. Item must belong to target, be droppable and not plot. PCs require Players permission. Confirm the new ground item in the report.",
        Handler::Command("drop_item"),
        &["area", "target", "item"],
        json!({"area":area(),"target":target(),"item":text("Item object ID from view_inventory",1,16)}),
    );
    add(
        "live_take_item",
        "Remove an inventory item",
        "Destroy one selected item, including its entire stack, from this holder. Supply item ID or resref exclusively; resref selects the first match. Plot items are protected.",
        Handler::Command("take_item"),
        &["area", "target"],
        json!({"area":area(),"target":target(),"item":target(),"resref":text("Item resref, mutually exclusive with item.",1,16)}),
    );
    add(
        "live_set_time",
        "Set game time",
        "Advance native time to hour/minute (earlier time advances the day). Minutes are civil 0-59, converted for the module time scale.",
        Handler::Command("set_time"),
        &["area", "hour", "minute"],
        json!({"area":area(),"hour":{"type":"integer","description":"Hour.","minimum":0,"maximum":23},"minute":{"type":"integer","description":"Minute.","minimum":0,"maximum":59}}),
    );
    add(
        "live_set_date",
        "Set game date",
        "Advance the native calendar; past dates are refused. NWN months all have 28 days.",
        Handler::Command("set_date"),
        &["area", "year", "month", "day"],
        json!({"area":area(),"year":{"type":"integer","description":"Year.","minimum":0,"maximum":32000},"month":{"type":"integer","description":"Month.","minimum":1,"maximum":12},"day":{"type":"integer","description":"Day.","minimum":1,"maximum":28}}),
    );
    add(
        "live_apply_vfx",
        "Apply visual effect",
        "Apply a tagged visualeffects.2da row to target OR location xyz in the area. Lasting durations require a D row. PCs require player permissions. Result records submission; lasting creature VFX appear in vfx, instant rendering needs a client.",
        Handler::Command("apply_vfx"),
        &["area", "row", "duration"],
        json!({"area":area(),"target":target(),"location":{"type":"object","description":"Location instead of target.","properties":{"x":field("number","X metres"),"y":field("number","Y metres"),"z":field("number","Z metres")},"required":["x","y","z"],"additionalProperties":false},
        "row":{"type":"integer","description":"visualeffects.2da row","minimum":0,"maximum":65535},
        "duration":{"type":"string","description":"Effect duration","enum":["instant","temporary","permanent"]},
        "seconds":{"type":"number","description":"Required only for temporary effects","exclusiveMinimum":0,"maximum":86400},"scale":{"type":"number","description":"Native effect scale, default 1","minimum":0.01,"maximum":100},"offset":vector("Native effect translation (metres), default zero",1000.),"rotation":vector("Native effect rotation XYZ, default zero",3600.)}),
    );
    add(
        "live_clear_vfx",
        "Clear owned visual effects",
        "Remove only effects tagged by live_apply_vfx. Optional row filters; omitted removes all ours. Result value.removed gives count. Unrelated script effects remain.",
        Handler::Command("clear_vfx"),
        &["area", "target"],
        json!({"area":area(),"target":target(),"row":{"type":"integer","description":"Optional visualeffects.2da row filter","minimum":0,"maximum":65535}}),
    );
    add(
        "live_replace_object_animation",
        "Replace object animation",
        "Map an animation name to another on the object's model. Reset one mapping with reset=true and no new value. Result value.state contains bounded native AnimationReplace storage; rendering requires suitable model assets. PCs require player permissions.",
        Handler::Command("replace_object_animation"),
        &["area", "target", "old"],
        json!({"area":area(),"target":target(),"old":text("Original animation name",1,16),"new":text("Replacement; excludes reset",1,16),"reset":field("boolean","Restore original; excludes new")}),
    );
    add(
        "live_set_static",
        "Set placeable static flag",
        "Changes the native placeable static flag. Clients update it only after re-entering the area. Read static in result value and area reports. Doors/creatures refused.",
        Handler::Command("set_static"),
        &["area", "target", "static"],
        json!({"area":area(),"target":target(),"static":field("boolean","Native static flag; client re-entry required")}),
    );
    add(
        "live_set_lootable",
        "Set NPC corpse loot",
        "Set lootability before death. Optional no_drop disables drops on current inventory/equipment, saving prior flags; false restores only flags this operation owns. New items are unaffected. PCs are refused. Read back loot in area state; result item_updates counts native updates.",
        Handler::Command("set_lootable"),
        &["area", "target", "lootable"],
        json!({"area":area(),"target":target(),"lootable":field("boolean","Leave a lootable corpse"),"no_drop":field("boolean","Optional: disable drops, or restore only our saved flags")}),
    );
    add(
        "live_set_appearance",
        "Set creature or placeable appearance",
        "Set an appearance.2da row for creatures/PCs or placeables.2da row for placeables. Verify app in the next area report. PCs need player permissions. Does not edit equipment/body-part fields.",
        Handler::Command("set_appearance"),
        &["area", "target", "row"],
        json!({"area":area(),"target":target(),"row":{"type":"integer","description":"appearance.2da or placeables.2da row","minimum":0,"maximum":65535}}),
    );
    add(
        "live_capture_object",
        "Capture object JSON",
        "Capture native ObjectToJson without saved runtime state, at most 256 KiB. Result object_json is the snapshot; live_wait returns it. Player/DM capture is refused. Capture requires read scope.",
        Handler::Query("capture_object"),
        &["area", "target"],
        json!({"area":area(),"target":target()}),
    );
    if policy.scripts {
        add(
            "live_spawn_json",
            "Spawn captured object",
            "JsonToObject from checked native GFF JSON, maximum 256 KiB / depth 32. Supports NPCs, placeables, doors and items; player/DM data refused. Requires spawn AND scripts groups, and scripts enabled in Options. Result obj is the new identity; verify it in area reports.",
            Handler::Command("spawn_json"),
            &["area", "json", "x", "y", "z", "f"],
            json!({"area":area(),"json":{"type":"object","description":"Bounded native GFF snapshot returned as capture_object.object_json, validated by the GFF reader.","additionalProperties":true},"x":field("number","X metres"),"y":field("number","Y metres"),"z":field("number","Z metres"),"f":field("number","Facing degrees")}),
        );
    }
    add(
        "live_patrol",
        "Patrol waypoints",
        "Walk an NPC through 1-16 xyz points in this area, optionally loop/run. Cancels the previous order; holds against AI routines. New move/attack/speech/rest/script orders or stop_patrol cancel it. Patrol events report actual arrivals; a point blocked for 120 seconds ends the patrol.",
        Handler::Command("patrol"),
        &["area", "target", "points"],
        json!({"area":area(),"target":target(),"points":{"type":"array","description":"Ordered route, 1-16 points.","minItems":1,"maxItems":16,"items":{"type":"object","description":"Waypoint xyz in metres.","additionalProperties":false,"required":["x","y","z"],"properties":{"x":field("number","X metres"),"y":field("number","Y metres"),"z":field("number","Z metres")}}},"loop":{"type":"boolean","description":"Repeat the route.","default":false},"run":{"type":"boolean","description":"Run instead of walking.","default":false}}),
    );
    add(
        "live_stop_patrol",
        "Stop patrol",
        "Cancel the owned patrol, clear its movement and restore commandability. Idempotent. Does not restart a module's routines explicitly.",
        Handler::Command("stop_patrol"),
        &["area", "target"],
        json!({"area":area(),"target":target()}),
    );
    add(
        "live_set_tile",
        "Change live terrain tile",
        "Set one active area tile. XY are tile grid coordinates, not metres. Checks area bounds and active tileset resource; recomputes lighting, grass and edge border. Native readback and a tile event carry before/after. Non-creature heights and adjoining terrain are not adjusted automatically.",
        Handler::Command("set_tile"),
        &["area", "x", "y", "tile_id", "orientation", "height"],
        json!({"area":area(),"x":{"type":"integer","description":"Tile column","minimum":0,"maximum":65535},"y":{"type":"integer","description":"Tile row","minimum":0,"maximum":65535},"tile_id":{"type":"integer","description":"ID in the active tileset .set file","minimum":0,"maximum":65535},"orientation":{"type":"integer","description":"Quarter-turns anticlockwise","minimum":0,"maximum":3},"height":{"type":"integer","description":"Native tile elevation units","minimum":0,"maximum":255}}),
    );
    if policy.players {
        add(
            "live_reply_to_player",
            "Reply to a player's latest speech",
            "Both player switches required. If the player's latest non-silent speech was to the DM, submit a private native DM-channel reply (30); otherwise the named NPC, or nearest living unprotected NPC within 20m, speaks in talk. Area is resolved from player. live_wait returns speaker, name, channel, route, recipient, submitted; submission is not proof of client delivery.",
            Handler::Command("reply_to_player"),
            &["player", "text"],
            json!({"player":target(),"text":text("Nonblank reply in the server's codepage.",1,200),"as":text("Optional NPC speaker ID for ordinary talk; ignored for DM replies.",1,16)}),
        );
        add(
            "live_pc_message",
            "Message one player",
            "Requires both player switches. Chat is private server feedback; dm prefixes it DM: and reports submission, not delivery (no DM avatar/channel spoof). Floating text is local; NUI falls back to chat, closing on OK or after 30 seconds. Other modes return value.nui_token, zero for non-NUI/fallback.",
            Handler::Command("pc_message"),
            &["area", "target", "text", "mode"],
            json!({"area":area(),"target":target(),"text":text("Message",1,200),"mode":{"type":"string","description":"Player display","enum":["chat","dm","floating","nui"]}}),
        );
        add(
            "live_pc_journal",
            "Set player journal entry",
            "Set this player's quest state, allowing lower-state override. No party broadcast. Quest must exist in the module journal; actual journal display requires a connected client.",
            Handler::Command("pc_journal"),
            &["area", "target", "quest", "state"],
            json!({"area":area(),"target":target(),"quest":text("Module journal quest tag (ASCII letters/digits/_/-)",1,64),"state":{"type":"integer","description":"Quest state, including lower-state override","minimum":0,"maximum":2147483647}}),
        );
        add(
            "live_pc_take_item",
            "Take an item from a player",
            "Requires both player switches. Exactly one item ID or resref; removes one matching item/stack. Plot items refused. Refresh view_inventory after completion.",
            Handler::Command("pc_take_item"),
            &["area", "target"],
            json!({"area":area(),"target":target(),"item":field("string","Inventory object ID, excludes resref"),"resref":text("Item resref, excludes item",1,16)}),
        );
        add(
            "live_pc_boot",
            "Disconnect a player",
            "Requires both player switches. UI must confirm the named target. Schedules BootPC after acknowledgement, with the optional reason shown to the player.",
            Handler::Command("pc_boot"),
            &["area", "target"],
            json!({"area":area(),"target":target(),"reason":text("Optional disconnection reason",0,200)}),
        );
        add(
            "live_fade",
            "Fade a player's screen",
            "Requires both player switches. Screen transition for one PC; connected-client rendering is needed for visual verification.",
            Handler::Command("fade"),
            &["area", "target", "direction", "seconds"],
            json!({"area":area(),"target":target(),"direction":{"type":"string","description":"Transition","enum":["to_black","from_black"]},"seconds":{"type":"number","description":"Fade duration seconds","exclusiveMinimum":0,"maximum":60}}),
        );
        add(
            "live_play_sound",
            "Play a sound at a player",
            "Requires both player switches. Native PlaySound emits a mono WAV at the player's world position: nearby clients can also hear it. This is not a private audio channel.",
            Handler::Command("play_sound"),
            &["area", "target", "resref"],
            json!({"area":area(),"target":target(),"resref":text("Existing mono WAV sound resref",1,16)}),
        );
        add(
            "live_skill_check",
            "Check nearby players' skill",
            "Supply target (one living PC) OR x/y/radius exclusively. Requires both player switches. Opens an NUI request; only the player Roll click consumes d20 + GetSkillRank(FALSE) + bonus. DM/possessed/dead PCs excluded. Locked DM bonus, optional displayed DC, optional outcome text. At most 64 recipients and 16 concurrent requests. Results update under the same id: pending=true while waiting, then rolled/no_roll/busy per player. Timeout defaults to 120 seconds. No NUI means chat notice then no_roll, never an automatic roll.",
            Handler::Command("skill_check"),
            &["area", "skill", "dc"],
            json!({"area":area(),"target":field("string","One PC object ID; omit x/y/radius"),"x":field("number","Centre X metres"),"y":field("number","Centre Y metres"),"radius":{"type":"number","description":"XY radius metres","exclusiveMinimum":0,"maximum":30},"skill":{"type":"integer","description":"skills.2da row","minimum":0,"maximum":65535},"dc":{"type":"integer","description":"Difficulty class","minimum":0,"maximum":255},"bonus":{"type":"integer","description":"DM bonus (locked for the player), default 0","minimum":-100,"maximum":100},"show_dc":field("boolean","Show DC to player, default true"),"timeout":{"type":"integer","description":"Seconds to await the player click, default 120","minimum":1,"maximum":600},"success_text":text("Passed window text; empty omits extra text.",0,200),"fail_text":text("Failed window text; empty omits extra text.",0,200)}),
        );
        let [x, y, z] = point();
        add(
            "live_jump_all_players",
            "Gather players",
            "Jump living connected PCs (not DMs/possessed) to this area/point. Requires both player switches; returns affected IDs.",
            Handler::Command("jump_all_players"),
            &["area", "x", "y", "z"],
            json!({"area":area(),"x":x,"y":y,"z":z}),
        );
    }
    add(
        "live_disable_trap",
        "Disable trap",
        "Disable the native trap on a door/placeable. Read dispatch with live_wait.",
        Handler::Command("disable_trap"),
        &["area", "target"],
        json!({"area":area(),"target":target()}),
    );
    add(
        "live_spawn_trap_on_object",
        "Trap a door or placeable",
        "Attach a traps.2da row to a door/placeable. Refuses active traps; the game validates row existence.",
        Handler::Command("spawn_trap_on_object"),
        &["area", "target", "trap"],
        json!({"area":area(),"target":target(),"trap":{"type":"integer","description":"Row in the running module's traps.2da.","minimum":0,"maximum":65535}}),
    );
    if policy.players {
        add(
            "live_give_level",
            "Grant next-level XP",
            "Grant XP up to the next exptable.2da threshold for current hit dice. Does not choose a class or bypass level-up. Refuses already eligible/max-level targets.",
            Handler::Command("give_level"),
            &["area", "target"],
            json!({"area":area(),"target":target()}),
        );
        let delta = || json!({"type":"integer","description":"Signed shift; positive towards law/good, negative towards chaos/evil.","minimum":-100,"maximum":100});
        add(
            "live_give_alignment",
            "Shift alignment",
            "Apply both signed deltas to this creature only; native values clamp to 0-100. Requires Players.",
            Handler::Command("give_alignment"),
            &["area", "target", "law_chaos", "good_evil"],
            json!({"area":area(),"target":target(),"law_chaos":delta(),"good_evil":delta()}),
        );
        add(
            "live_set_stat",
            "Set raw ability",
            "Set an unmodified raw ability score, excluding racial/effect modifiers; live_wait returns its native value. Requires Players.",
            Handler::Command("set_stat"),
            &["area", "target", "ability", "value"],
            json!({"area":area(),"target":target(),"ability":{"type":"string","description":"Ability selector.","enum":["strength","dexterity","constitution","intelligence","wisdom","charisma"]},"value":{"type":"integer","description":"Raw native score.","minimum":0,"maximum":255}}),
        );
    }
    add(
        "live_get_faction_reputation",
        "Read faction reputation",
        "Read how the specified faction feels about this creature. Custom factions need an extant representative. NPCs do not require Players; protected targets require both player switches.",
        Handler::Query("get_faction_reputation"),
        &["area", "target", "faction"],
        json!({"area":area(),"target":target(),"faction":faction()}),
    );
    add(
        "live_set_faction_reputation",
        "Set faction reputation",
        "Set how this faction feels about the creature; affects native faction standings. Custom factions use AdjustReputation; live_wait returns actual before/after. NPCs do not require Players; protected targets require both player switches.",
        Handler::Command("set_faction_reputation"),
        &["area", "target", "faction", "reputation"],
        json!({"area":area(),"target":target(),"faction":faction(),"reputation":{"type":"integer","description":"Desired reputation.","minimum":0,"maximum":100}}),
    );
    let local_target =
        || text("Object ID, or module/area for the module/current area locals.", 1, 16);
    let local_type = || json!({"type":"string","description":"NWScript local type.","enum":["int","float","string","object","location"]});
    let local_name = || {
        text(
            "ASCII local name (letters, digits, space, _.-); MGLIVE_/NWNX_ prefixes are reserved.",
            1,
            128,
        )
    };
    add(
        "live_get_variable",
        "Read a local",
        "Read one typed local asynchronously; live_wait returns value. Unset values use native defaults.",
        Handler::Query("get_variable"),
        &["area", "target", "type", "name"],
        json!({"area":area(),"target":local_target(),"type":local_type(),"name":local_name()}),
    );
    add(
        "live_set_variable",
        "Write a local",
        "Write an application local. Reserved transport/plugin locals are protected. Read normalized value with live_wait.",
        Handler::Command("set_variable"),
        &["area", "target", "type", "name", "value"],
        json!({"area":area(),"target":local_target(),"type":local_type(),"name":local_name(),"value":{"description":"int: signed 32-bit; float: finite 32-bit; string: <=4096 characters; object: hex ID/null; location: area,x,y,z,f object/null.","oneOf":[{"type":"number"},{"type":"string","maxLength":4096},{"type":"null"},{"type":"object","additionalProperties":false,"required":["area","x","y","z","f"],"properties":{"area":area(),"x":field("number","X."),"y":field("number","Y."),"z":field("number","Z."),"f":field("number","Facing.")}}]}}),
    );
    add(
        "live_dump_locals",
        "List locals",
        "Read up to 128 locals/15KB, examining at most 256 native entries. Reserved locals are omitted; result includes total, omitted, truncated. Native ordering is unstable.",
        Handler::Query("dump_locals"),
        &["area", "target"],
        json!({"area":area(),"target":local_target()}),
    );
    let [x, y, z] = point();
    add(
        "live_spawn",
        "Spawn an object",
        "Spawn a creature, placeable or item; read its result and new object id with live_wait.",
        Handler::Command("spawn"),
        &["area", "kind", "r", "x", "y", "z", "f"],
        json!({"area":area(),"kind":{"type":"string","description":"Object kind.","enum":["creature","placeable","item","waypoint","sound","encounter","store","trigger"]},"r":text("Blueprint resref.",1,16),"x":x,"y":y,"z":z,"f":field("number","Facing in degrees."),
            "count":{"type":"integer","description":"Number to spawn, spread on a two-metre grid; native placement can snap to the walkmesh.","minimum":1,"maximum":10,"default":1},"faction":faction()}),
    );
    add(
        "live_destroy",
        "Destroy an object",
        "Destroy a non-player, non-DM object. Read dispatch result with live_wait.",
        Handler::Command("destroy"),
        &["area", "target"],
        json!({"area":area(),"target":target()}),
    );
    let [x, y, z] = point();
    add(
        "live_npc_move",
        "Move an NPC",
        "Order a non-player, non-DM creature to walk or run. Read dispatch with live_wait and observed position with live_area_state.",
        Handler::Command("npc_move"),
        &["area", "target", "x", "y", "z"],
        json!({"area":area(),"target":target(),"x":x,"y":y,"z":z,"run":{"type":"boolean","description":"Run instead of walking.","default":false},
            "hold":{"type":"boolean","description":"Prevent routines replacing movement until arrival, a new movement/attack order, or timeout.","default":false},
            "timeout":{"type":"integer","description":"Maximum hold duration in seconds.","minimum":1,"maximum":120,"default":30}}),
    );
    add(
        "live_npc_say",
        "NPC speech",
        "Make a non-player, non-DM creature speak. Read dispatch result with live_wait.",
        Handler::Command("npc_say"),
        &["area", "target", "text"],
        json!({"area":area(),"target":target(),"text":text("Nonblank text to speak, up to 200 game-codepage characters.",1,200),
            "volume":{"type":"string","description":"Speech volume.","enum":["talk","whisper","shout"],"default":"talk"}}),
    );
    add(
        "live_npc_attack",
        "NPC attack",
        "Order a non-player, non-DM attacker to fight. Same-faction attacks temporarily use a different faction (Commoner against Hostile; otherwise Hostile), or a specified fighting faction, always with a bounded hold; result faction gives original/temporary IDs. Its original faction returns at hold end, victim death, timeout or a new order. Different factions use temporary personal hostility. Confirm combat/target in live_area_state.",
        Handler::Command("npc_attack"),
        &["area", "target", "victim"],
        json!({"area":area(),"target":target(),"victim":target(),"faction":faction(),
            "hold":{"type":"boolean","description":"Prevent routines replacing the attack until it ends, the victim dies, a new order arrives, or timeout. Always enabled for a same-faction attack.","default":false},
            "timeout":{"type":"integer","description":"Maximum hold duration in seconds.","minimum":1,"maximum":120,"default":30}}),
    );
    add(
        "live_set_state",
        "Object state",
        "Open/close or lock/unlock a supported object; supply at least one state. Read dispatch result with live_wait.",
        Handler::Command("set_state"),
        &["area", "target"],
        json!({"area":area(),"target":target(),"open":field("boolean","Desired open state."),"lock":field("boolean","Desired locked state.")}),
    );
    add(
        "live_stop_combat",
        "Stop a selected attack or calm nearby NPCs",
        "Supply either one target or all of x/y/radius, never both. Clear selected NPC actions and make them temporarily friendly for 180 seconds. Target mode also holds against AI until 180 seconds or a new order. A targeted PC requires both player switches and only clears actions/releases our hold, without faction/reputation changes or an AI calm hold. DM/possessed targets remain protected. Protected associates need both player switches. Radius mode continues to skip direct players. Read affected IDs with live_wait and combat state with live_area_state.",
        Handler::Command("stop_combat"),
        &["area"],
        json!({"area":area(),"target":target(),"x":field("number","Centre X in metres."),"y":field("number","Centre Y in metres."),
            "radius":{"type":"number","description":"Radius in metres, greater than zero and at most 50.","exclusiveMinimum":0,"maximum":50}}),
    );
    add(
        "live_set_faction",
        "Set NPC faction",
        "Change a living NPC's faction; the result returns its numeric ID. Read faction in live_area_state. Player-owned associates require both player switches.",
        Handler::Command("set_faction"),
        &["area", "target", "faction"],
        json!({"area":area(),"target":target(),"faction":faction()}),
    );
    add(
        "live_freeze",
        "Freeze a creature",
        "Apply a tagged temporary cutscene paralysis. Read frozen in live_area_state. PCs and their associates require both player switches.",
        Handler::Command("freeze"),
        &["area", "target", "seconds"],
        json!({"area":area(),"target":target(),"seconds":{"type":"number","description":"Duration in seconds, greater than zero and at most 300.","exclusiveMinimum":0,"maximum":300}}),
    );
    add(
        "live_unfreeze",
        "Unfreeze a creature",
        "Remove only Moonglow's tagged freeze effects. Read frozen in live_area_state; unrelated paralysis is preserved.",
        Handler::Command("unfreeze"),
        &["area", "target"],
        json!({"area":area(),"target":target()}),
    );
    if policy.players {
        add(
            "live_pc_heal",
            "Heal a player",
            "Heal a living creature. Requires local player permission AND server MGLIVE_ALLOW_PLAYER_OPS=1. Confirm HP in live_area_state.",
            Handler::Command("pc_heal"),
            &["area", "target"],
            json!({"area":area(),"target":target(),"amount":{"type":"integer","description":"HP to heal; omitted: maximum HP.","minimum":1,"maximum":100000}}),
        );
        add(
            "live_pc_raise",
            "Raise a player",
            "Resurrect a dead creature. Requires both player switches. Confirm dead:false and HP in live_area_state.",
            Handler::Command("pc_raise"),
            &["area", "target"],
            json!({"area":area(),"target":target()}),
        );
        let [x, y, z] = point();
        add(
            "live_pc_teleport",
            "Teleport a player",
            "Compatibility alias for teleport on creatures. Optional area is the destination; omission uses the current area. Requires both player switches. Cancels holds/patrols; native placement can snap to walkmesh.",
            Handler::Command("pc_teleport"),
            &["target", "x", "y", "z"],
            json!({"area":area(),"target":target(),"x":x,"y":y,"z":z,"f":{"type":"number","description":"Optional facing; omitted preserves it","minimum":0,"exclusiveMaximum":360}}),
        );
        add(
            "live_pc_give_gold",
            "Give gold",
            "Add gold to a living creature without overflowing its total. Requires both player switches. Confirm gold in live_area_state.",
            Handler::Command("pc_give_gold"),
            &["area", "target", "amount"],
            json!({"area":area(),"target":target(),"amount":{"type":"integer","description":"Gold to add.","minimum":1,"maximum":1000000}}),
        );
        add(
            "live_pc_give_xp",
            "Give experience",
            "Add XP to a living creature without overflowing its total. Requires both player switches. Confirm xp in live_area_state.",
            Handler::Command("pc_give_xp"),
            &["area", "target", "amount"],
            json!({"area":area(),"target":target(),"amount":{"type":"integer","description":"Experience to add.","minimum":1,"maximum":100000}}),
        );
        add(
            "live_pc_give_item",
            "Give an item",
            "Create one item in the creature's inventory. Requires both player switches. Result obj is the created item ID; inventory contents are not in area reports.",
            Handler::Command("pc_give_item"),
            &["area", "target", "resref"],
            json!({"area":area(),"target":target(),"resref":text("Existing item blueprint resref.",1,16)}),
        );
    }
    add(
        "live_set_facing",
        "Set placeable facing",
        "Set a placeable's true facing. Doors return disabled because the native setter ignores them; visual rotation is supported separately.",
        Handler::Command("set_facing"),
        &["area", "target", "f"],
        json!({"area":area(),"target":target(),"f":{"type":"number","description":"Facing in degrees, from zero inclusive to 360 exclusive.","minimum":0,"exclusiveMaximum":360}}),
    );
    add(
        "live_set_visual_transform",
        "Transform a placeable or door",
        "Change BASE visual scale/rotation/translation; omitted groups retain their values. Supply at least one group, or reset:true alone. Read vt on the next full area report; absent vt means identity. Does not change collision geometry.",
        Handler::Command("set_visual_transform"),
        &["area", "target"],
        json!({"area":area(),"target":target(),"scale":{"type":"number","description":"Uniform scale greater than zero, up to 10.","exclusiveMinimum":0,"maximum":10},
            "rotate":vector("Rotation in degrees; all three axes required.",360.0),"translate":vector("Visual translation in metres; all three axes required.",1000.0),
            "reset":{"type":"boolean","description":"Clear the BASE transform; cannot be combined with transform groups.","default":false}}),
    );
    let [x, y, z] = point();
    add(
        "live_set_position",
        "Move a placeable or door",
        "Set true position with NWNX_Object, updating subareas and preserving facing. Confirm coordinates on the next full area report.",
        Handler::Command("set_position"),
        &["area", "target", "x", "y", "z"],
        json!({"area":area(),"target":target(),"x":x,"y":y,"z":z}),
    );
    add(
        "live_play_animation",
        "Animate a placeable or door",
        "Play a model-supported placeable animation. Doors accept only open/close; opening a locked door is invalid. Confirm native anim (and door open) on the next full report.",
        Handler::Command("play_animation"),
        &["area", "target", "name"],
        json!({"area":area(),"target":target(),"name":{"type":"string","description":"Animation name.","enum":["activate","deactivate","open","close"]}}),
    );
    add(
        "live_set_useable",
        "Set object useability",
        "Change a non-static placeable or door's useable flag. Static placeables return disabled. Read useable on the next full report.",
        Handler::Command("set_useable"),
        &["area", "target", "useable"],
        json!({"area":area(),"target":target(),"useable":field("boolean","Desired native useability.")}),
    );
    add(
        "live_set_name",
        "Rename an object",
        "Rename a non-player creature, placeable, door or item. Confirm name in the next report containing that object. The server codepage must support the characters.",
        Handler::Command("set_name"),
        &["area", "target", "name"],
        json!({"area":area(),"target":target(),"name":text("Nonblank name, up to 200 game-codepage characters.",1,200)}),
    );

    for (name, title, description) in [
        (
            "kill",
            "Kill a creature",
            "Apply instant supernatural EffectDeath, temporarily overriding and restoring plot/immortal flags. Result value contains dead and overridden flags; unsuccessful death returns invalid. Players need all player permissions.",
        ),
        (
            "force_rest",
            "Force a rest",
            "Native ForceRest on a living creature. Confirm HP/spell recovery; PCs require player permissions.",
        ),
        (
            "limbo",
            "Send a creature to limbo",
            "Remove an NPC from the area into module limbo; identity remains valid. PCs/DMs are never supported by the native limbo API.",
        ),
    ] {
        add(
            &format!("live_{name}"),
            title,
            description,
            Handler::Command(name),
            &["area", "target"],
            json!({"area":area(),"target":target()}),
        );
    }
    for (name, title, field_name, limit, minimum, description) in [
        (
            "take_gold",
            "Take gold",
            "amount",
            1_000_000,
            1,
            "Destroy up to amount gold; clamps at zero. NPCs permitted; PCs require Players permission.",
        ),
        (
            "take_xp",
            "Take XP",
            "amount",
            100_000,
            1,
            "Subtract XP, clamped at zero, using native SetXP. May remove levels. PCs require Players permission.",
        ),
        (
            "set_xp",
            "Set XP",
            "value",
            i32::MAX,
            0,
            "Set absolute native XP; may remove levels. PCs require Players permission.",
        ),
    ] {
        add(
            &format!("live_{name}"),
            title,
            description,
            Handler::Command(name),
            &["area", "target", field_name],
            json!({"area":area(),"target":target(),field_name:{"type":"integer","description":description,"minimum":minimum,"maximum":limit}}),
        );
    }
    add(
        "live_remove_level",
        "Remove one level",
        "Set XP to the previous actual level's exptable threshold. Native SetXP handles level loss. Level one is refused. PCs require Players permission.",
        Handler::Command("remove_level"),
        &["area", "target"],
        json!({"area":area(),"target":target()}),
    );
    add(
        "live_play_creature_animation",
        "Play a creature animation",
        "Play a numeric NWScript creature ANIMATION_* constant or custom model animation name. NPCs allowed; PCs require Players permission. loop repeats fire-and-forget animations until seconds elapse; looping constants use their native timed behavior. Custom names use a temporary animation replacement, restored on timeout or a new live order. Native model availability/rendering requires client assets.",
        Handler::Command("play_creature_animation"),
        &["area", "target", "animation"],
        json!({"area":area(),"target":target(),"animation":{"description":"Creature ANIMATION_* numeric constant, or model animation resref","oneOf":[{"type":"integer","minimum":0,"maximum":116},{"type":"string","minLength":1,"maxLength":16}]},"loop":{"type":"boolean","description":"Repeat until seconds expire","default":false},"speed":{"type":"number","description":"Playback multiplier","minimum":0.1,"maximum":10,"default":1},"seconds":{"type":"number","description":"Maximum time; native one-shot may finish earlier","minimum":0.1,"maximum":300,"default":30}}),
    );
    add(
        "live_heal",
        "Heal a creature",
        "Heal an NPC without player switches, or a PC with all player permissions. Omitted amount fills HP; not resurrection.",
        Handler::Command("heal"),
        &["area", "target"],
        json!({"area":area(),"target":target(),"amount":{"type":"integer","description":"HP restored, omitted: maximum HP.","minimum":1,"maximum":100000}}),
    );
    for (name, title, description) in [
        (
            "set_invulnerable",
            "Set invulnerability",
            "Set the creature's native plot flag; explicit boolean, not a toggle.",
        ),
        (
            "set_immortal",
            "Set immortality",
            "Set the native immortal flag; explicit boolean, not a toggle.",
        ),
    ] {
        add(
            &format!("live_{name}"),
            title,
            description,
            Handler::Command(name),
            &["area", "target", "enabled"],
            json!({"area":area(),"target":target(),"enabled":field("boolean","Desired flag value.")}),
        );
    }
    add(
        "live_set_ai",
        "Set AI level",
        "Set the creature's native AI level; default lets the engine select its level.",
        Handler::Command("set_ai"),
        &["area", "target", "level"],
        json!({"area":area(),"target":target(),"level":{"type":"string","description":"Native AI_LEVEL selector.","enum":["default","very_low","low","normal","high","very_high"]}}),
    );
    let [x, y, z] = point();
    add(
        "live_unlimbo",
        "Return a creature from limbo",
        "Jump a valid NPC from module limbo to a reported destination area. Its ID is deliberately absent from area reports while in limbo; the game checks identity and membership.",
        Handler::Command("unlimbo"),
        &["area", "target", "x", "y", "z"],
        json!({"area":area(),"target":target(),"x":x,"y":y,"z":z}),
    );
    if policy.scripts {
        add(
            "live_run_script",
            "Run a module script",
            "Run a compiled module script on the target, or on the area when omitted. Read result with live_wait.",
            Handler::Command("run_script"),
            &["area", "script"],
            json!({"area":area(),"script":text("Compiled script resref: ASCII letters, digits or underscore.",1,16),"target":target()}),
        );
    }
    if policy.chunks {
        add(
            "live_run_chunk",
            "Run NWScript",
            "Run arbitrary NWScript on the target, or on the area when omitted. Also requires server MGLIVE_ALLOW_CHUNK=1. Read success or compiler error with live_wait.",
            Handler::Command("run_chunk"),
            &["area", "code"],
            json!({"area":area(),"code":text("NWScript source; logged in abbreviated form.",1,8000),"target":target(),"wrap":{"type":"boolean","description":"Wrap the source in void main().","default":true},"return":{"type":"boolean","description":"Include MgReturn(string JSON) and return its last synchronous JSON value (at most 16384 ASCII JSON bytes); no call returns null.","default":false}}),
        );
    }
    add(
        "live_spawn_trigger",
        "Spawn generic trigger",
        "Create an owned trigger; optional r selects a UTT blueprint, otherwise script-free generic. Circle uses radius; rectangle width/height, exclusive. Vertices must remain inside the area.",
        Handler::Command("spawn_trigger"),
        &["area", "x", "y", "z", "f", "shape"],
        json!({"area":area(),"x":field("number","Centre X metres."),"y":field("number","Centre Y metres."),"z":field("number","Centre Z metres."),"f":field("number","Facing degrees."),"r":text("Optional UTT blueprint resref; its scripts and properties are preserved.",1,16),"shape":{"type":"string","enum":["circle","rectangle"],"description":"Circle (16 vertices) or rectangle."},"radius":{"type":"number","exclusiveMinimum":0,"maximum":30,"description":"Circle radius metres."},"width":{"type":"number","exclusiveMinimum":0,"maximum":30,"description":"Rectangle width metres."},"height":{"type":"number","exclusiveMinimum":0,"maximum":30,"description":"Rectangle height metres."}}),
    );
    ops.extend(atmosphere::operations());
    for op in &mut ops {
        if matches!(op.handler, Handler::Command(_) | Handler::Query(_)) {
            op.descriptor["input"]["properties"]["id"] =
                text("Optional idempotency id; reuse only for the identical command.", 1, 64);
        }
    }
    ops.extend(assistant::operations());
    ops
}

pub fn describe(policy: Policy) -> Value {
    json!({"protocol":"moonglow-agent/1","moonglow":{"name":"Moonglow","version":env!("CARGO_PKG_VERSION"),"build":option_env!("MOONGLOW_BUILD").unwrap_or(""),"variant":"upstream"},
        "instructions":format!("The live group controls the running game over Redis. Read areas from live_status and hexadecimal object IDs from live_area_state. Commands are asynchronous: use live_wait with the queued ID, then read area state to confirm effects. Script execution is {}; NWScript chunks are {}. Player operations are {} (also require server MGLIVE_ALLOW_PLAYER_OPS=1). Editor operations are not available in this version.",if policy.scripts {"enabled"} else {"disabled"},if policy.chunks {"enabled and also requires the server switch"} else {"disabled"},if policy.players {"enabled"} else {"disabled"}),
        "capabilities":{"eventStream":{"operation":"live_events"}},
        "operations":table(policy).iter().map(|op|&op.descriptor).collect::<Vec<_>>()})
}

/// Enforce the advertised subset, including unknown-property rejection.
fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    if let Some(options) = schema["oneOf"].as_array() {
        return if options.iter().filter(|s| validate(s, value).is_ok()).count() == 1 {
            Ok(())
        } else {
            Err("Value must match exactly one allowed type".into())
        };
    }
    let valid = match schema["type"].as_str() {
        Some("object") => value.is_object(),
        Some("string") => value.is_string(),
        Some("number") => value.is_number(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("boolean") => value.is_boolean(),
        Some("array") => value.is_array(),
        Some("null") => value.is_null(),
        _ => false,
    };
    if !valid {
        return Err(format!("Expected {}", schema["type"]));
    }
    if let Some(object) = value.as_object() {
        for key in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if !object.contains_key(key) {
                return Err(format!("Missing {key}"));
            }
        }
        for (key, item) in object {
            if let Some(property) = schema["properties"].get(key) {
                validate(property, item).map_err(|e| format!("{key}: {e}"))?;
            } else if schema["additionalProperties"] != true {
                return Err(format!("Unknown property {key}"));
            }
        }
    }
    if let Some(items) = value.as_array() {
        if schema["minItems"].as_u64().is_some_and(|n| (items.len() as u64) < n)
            || schema["maxItems"].as_u64().is_some_and(|n| (items.len() as u64) > n)
        {
            return Err("Array length is out of range".into());
        }
        if let Some(item_schema) = schema.get("items") {
            for item in items {
                validate(item_schema, item)?;
            }
        }
    }
    if let Some(s) = value.as_str() {
        let n = s.chars().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|min| n < min)
            || schema["maxLength"].as_u64().is_some_and(|max| n > max)
        {
            return Err("String length is out of range".into());
        }
    }
    if let Some(n) = value.as_f64()
        && (schema["minimum"].as_f64().is_some_and(|min| n < min)
            || schema["maximum"].as_f64().is_some_and(|max| n > max)
            || schema["exclusiveMinimum"].as_f64().is_some_and(|min| n <= min)
            || schema["exclusiveMaximum"].as_f64().is_some_and(|max| n >= max))
    {
        return Err("Number is out of range".into());
    }
    if let Some(options) = schema["enum"].as_array()
        && !options.contains(value)
    {
        return Err("Value is not in enum".into());
    }
    Ok(())
}

/// The original command endpoint uses the same catalogue validation as /v1/call.
#[cfg(test)]
pub(crate) fn command(
    input: Value,
    live: &LiveStore,
    now_ms: u64,
) -> Result<Value, crate::store::CommandError> {
    command_scoped(input, live, now_ms, &crate::access::Scope::default())
}
pub fn command_scoped(
    mut input: Value,
    live: &LiveStore,
    now_ms: u64,
    scope: &crate::access::Scope,
) -> Result<Value, crate::store::CommandError> {
    let kind = input.as_object_mut().and_then(|object| object.remove("op"));
    // Validate known commands even when disabled, then let enqueue retain the
    // specific Options refusal. Discovery still hides disabled operations.
    let op = table(Policy { scripts: true, chunks: true, players: true })
        .iter()
        .find(|op| {
            matches!(op.handler, Handler::Command(name) | Handler::Query(name)
                if kind.as_ref().and_then(Value::as_str) == Some(name))
        })
        .ok_or("Unknown command operation")?;
    op.validate_input(&input, scope)?;
    input["op"] = kind.unwrap();
    let command = serde_json::from_value(input).map_err(|e| e.to_string())?;
    live.enqueue_scoped(command, now_ms, scope).map(|id| json!({"queued":id}))
}

pub fn call(
    name: &str,
    input: Value,
    live: &LiveStore,
    stop: &AtomicBool,
) -> Result<Value, crate::store::CommandError> {
    call_scoped(name, input, live, stop, &crate::access::Scope::default())
}
pub fn call_scoped(
    name: &str,
    mut input: Value,
    live: &LiveStore,
    stop: &AtomicBool,
    scope: &crate::access::Scope,
) -> Result<Value, crate::store::CommandError> {
    let op =
        table(live.policy()).iter().find(|op| op.descriptor["name"] == name).ok_or_else(|| {
            crate::store::CommandError::new(
                crate::store::ErrorCode::Disabled,
                "Unknown or disabled operation",
            )
        })?;
    op.validate_input(&input, scope)?;
    match op.handler {
        Handler::Status => Ok(live.status_scoped(now_ms(), scope)),
        Handler::Area => live
            .area_json(input["area"].as_str().unwrap_or_default(), now_ms())
            .ok_or_else(|| "Unknown area".into()),
        Handler::Events => {
            let q = crate::events::Query {
                since: input["since"].as_u64().unwrap_or(0),
                area: input["area"].as_str().map(str::to_string),
                kinds: input["kinds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect(),
                object: input["object"].as_str().map(str::to_string),
                text: input["text"].as_str().map(str::to_string),
                limit: input["limit"].as_u64().unwrap_or(100) as usize,
            };
            let filter = crate::store::event_store::ChatFilter {
                players: input["from"] == "players",
                to_dm: input["to_dm"].as_bool(),
                boot: input["boot"].as_str().map(str::to_string),
            };
            let page =
                live.wait_events(&q, scope, &filter, input["wait_ms"].as_u64().unwrap_or(0), stop)?;
            Ok(json!(page))
        }
        Handler::Assistant(name) => assistant::call(name, &input, live, scope),
        Handler::Find => {
            let limit = input["limit"].as_u64().unwrap_or(50) as usize;
            Ok(json!({"objects": live.find(input["text"].as_str().unwrap_or_default(), limit)}))
        }
        Handler::Command(kind) | Handler::Query(kind) => {
            input["op"] = json!(kind);
            let command = serde_json::from_value(input).map_err(|e| e.to_string())?;
            live.enqueue_scoped(command, now_ms(), scope).map(|id| json!({"queued":id}))
        }
        Handler::Wait => {
            let id = input["id"].as_str().unwrap_or_default();
            let deadline = Instant::now()
                + Duration::from_millis(input["timeout_ms"].as_u64().unwrap_or(5000));
            loop {
                if stop.load(Ordering::Acquire) {
                    return Err("Live service stopped".into());
                }
                if let Some(draft) = live.draft_result_scoped(id, scope) {
                    return Ok(serde_json::to_value(draft).unwrap_or(Value::Null));
                }
                let status = live.status_scoped(now_ms(), scope);
                if let Some(found) = status["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|r| r["result"]["id"] == id)
                {
                    return Ok(found["result"].clone());
                }
                if !status["queued"].as_array().into_iter().flatten().any(|q| q["id"] == id) {
                    return Err("Unknown command id or result no longer retained".into());
                }
                if Instant::now() >= deadline {
                    return Ok(json!({"id":id,"pending":true}));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

pub fn openapi(policy: Policy) -> Value {
    let mut paths = serde_json::Map::new();
    for op in table(policy) {
        let d = &op.descriptor;
        paths.insert(format!("/v1/call/{}",d["name"].as_str().unwrap_or_default()),json!({"post":{
            "operationId":d["name"],"summary":d["title"],"description":d["description"],"security":[{"bearer":[]}],
            "requestBody":{"required":true,"content":{"application/json":{"schema":d["input"]}}},
            "responses":{"200":{"description":"{ok:true,result:...}; commands return queued ids, wait returns a result or pending."},"400":{"description":"{ok:false,error:...,code:...}"},"401":{"description":"Missing or invalid bearer token."},"403":{"description":"LLM access is off ({err:...}), or client scope forbids this operation or command ID."},"429":{"description":"Named client command quota exceeded.","headers":{"Retry-After":{"description":"Seconds until the command window resets.","schema":{"type":"integer"}}}}}}}));
    }
    json!({"openapi":"3.1.0","info":{"title":"Moonglow agent API","version":"1"},"paths":paths,
        "components":{"securitySchemes":{"bearer":{"type":"http","scheme":"bearer"}}}})
}

#[cfg(test)]
#[path = "campaign_examples_tests.rs"]
mod campaign_examples_tests;

#[cfg(test)]
#[path = "measure_dispatch.rs"]
mod measurement;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_catalogues_preserve_every_policy() {
        for flags in 0..8 {
            let policy =
                Policy { scripts: flags & 1 != 0, chunks: flags & 2 != 0, players: flags & 4 != 0 };
            let cached = table(policy);
            let built = build_table(policy);
            assert!(std::ptr::eq(cached, table(policy)));
            assert_eq!(cached.len(), built.len());
            for (a, b) in cached.iter().zip(&built) {
                assert_eq!(a.descriptor, b.descriptor);
            }
        }
    }
    fn subset(s: &Value) {
        let allowed = [
            "type",
            "properties",
            "required",
            "items",
            "enum",
            "minimum",
            "maximum",
            "exclusiveMinimum",
            "exclusiveMaximum",
            "oneOf",
            "minLength",
            "maxLength",
            "minItems",
            "maxItems",
            "description",
            "default",
            "additionalProperties",
        ];
        for key in s.as_object().unwrap().keys() {
            assert!(allowed.contains(&key.as_str()), "{key}");
        }
        for option in s["oneOf"].as_array().into_iter().flatten() {
            subset(option);
        }
        if s["type"] == "object" {
            assert!(s["additionalProperties"].is_boolean());
            for property in s["properties"].as_object().into_iter().flat_map(|p| p.values()) {
                assert!(property["description"].as_str().is_some_and(|s| !s.is_empty()));
                subset(property);
            }
        }
    }
    #[test]
    fn catalogue_is_flat_and_policy_controls_dispatch() {
        let all = describe(Policy { scripts: true, chunks: true, players: true });
        assert_eq!(all["operations"].as_array().unwrap().len(), 96);
        for op in all["operations"].as_array().unwrap() {
            let name = op["name"].as_str().unwrap();
            assert!(name.len() <= 49 && name.bytes().next().unwrap().is_ascii_lowercase());
            assert!(
                name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            );
            subset(&op["input"]);
        }
        let live = LiveStore::default();
        assert!(!describe(live.policy()).to_string().contains("live_run_chunk"));
        assert!(describe(live.policy()).to_string().contains("live_get_faction_reputation"));
        assert!(describe(live.policy()).to_string().contains("live_set_faction_reputation"));
        let stop = AtomicBool::new(false);
        assert!(call("live_run_chunk", json!({}), &live, &stop).is_err());
        assert!(call("live_status", json!({"extra":true}), &live, &stop).is_err());
        assert!(call("live_wait", json!({"id":"x","timeout_ms":30001}), &live, &stop).is_err());
        live.set_policy(Policy { scripts: false, chunks: false, players: false });
        assert!(!describe(live.policy()).to_string().contains("live_run_script"));
    }

    #[test]
    fn reply_to_player_validates_gates_and_exact_game_command() {
        let live = LiveStore::default();
        let now = now_ms();
        live.sync(br#"{"v":1,"boot":"b","seq":1,"areas":[{"area":"room","cre":[{"id":"1","pc":true},{"id":"2"},{"id":"3","pc":true,"dm":true}]}]}"#,now).unwrap();
        let input = json!({"player":"1","text":"Hello","as":"2"});
        let stop = AtomicBool::new(false);
        assert_eq!(
            call("live_reply_to_player", input.clone(), &live, &stop).unwrap_err().code,
            crate::store::ErrorCode::Disabled
        );
        live.set_policy(Policy { players: true, ..Default::default() });
        for bad in [
            json!({"player":"bad-id","text":"hi"}),
            json!({"player":"1","text":" "}),
            json!({"player":"1","text":"x".repeat(201)}),
            json!({"player":"1","text":"hi","as":"bad-id"}),
            json!({"player":"1","text":"hi","area":"room"}),
        ] {
            assert_eq!(
                call("live_reply_to_player", bad, &live, &stop).unwrap_err().code,
                crate::store::ErrorCode::Invalid
            );
        }
        for player in ["2", "3"] {
            assert_eq!(
                call("live_reply_to_player", json!({"player":player,"text":"hi"}), &live, &stop)
                    .unwrap_err()
                    .code,
                crate::store::ErrorCode::Protected
            );
        }
        let queued = call("live_reply_to_player", input, &live, &stop).unwrap();
        assert_eq!(
            live.commands(now_ms())["commands"][0],
            json!({"id":queued["queued"],"op":"reply_to_player","player":"1","text":"Hello","as":"2","by":"default","players":true})
        );
        let op = table(live.policy())
            .iter()
            .find(|o| o.descriptor["name"] == "live_reply_to_player")
            .unwrap();
        assert!(
            op.validate_input(
                &json!({"player":"1","text":"hello"}),
                &crate::access::Scope::default()
            )
            .is_ok()
        );
    }
    #[test]
    fn waiting_returns_pending_results_and_interrupts_on_stop() {
        let live = std::sync::Arc::new(LiveStore::default());
        let report =
            json!({"v":1,"boot":"test","seq":1,"areas":[{"area":"room","full":true,"cre":[]}]});
        live.sync(report.to_string().as_bytes(), now_ms()).unwrap();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let queued = call(
            "live_run_script",
            json!({"area":"room","script":"example","id":"wait-test"}),
            &live,
            &stop,
        )
        .unwrap();
        assert_eq!(queued["queued"], "wait-test");
        assert_eq!(
            call("live_wait", json!({"id":"wait-test","timeout_ms":0}), &live, &stop).unwrap()["pending"],
            true
        );
        let (store, cancel) = (live.clone(), stop.clone());
        let worker = std::thread::spawn(move || {
            call("live_wait", json!({"id":"wait-test","timeout_ms":30000}), &store, &cancel)
        });
        stop.store(true, Ordering::Release);
        let start = Instant::now();
        assert!(worker.join().unwrap().unwrap_err().contains("stopped"));
        assert!(start.elapsed() < Duration::from_secs(1));
        stop.store(false, Ordering::Release);
        let result = json!({"v":1,"boot":"test","seq":2,"ack":["wait-test"],"results":[{"id":"wait-test","ok":true}]});
        live.sync(result.to_string().as_bytes(), now_ms()).unwrap();
        assert_eq!(call("live_wait", json!({"id":"wait-test"}), &live, &stop).unwrap()["ok"], true);
    }
    #[test]
    fn events_and_found_objects_come_from_reports() {
        let live = LiveStore::default();
        let stop = AtomicBool::new(false);
        let guard =
            json!({"id":"7","tag":"GUARD","r":"nw_guard","name":"Gate Guard","x":1.0,"y":2.0});
        let report = |seq: u64, cre: Value, events: Value| json!({"v":1,"boot":"ev","seq":seq,"areas":[{"area":"Gate","full":true,"cre":cre}],"events":events,"dropped":0});
        live.sync(report(1, json!([guard]), json!([])).to_string().as_bytes(), now_ms()).unwrap();
        let fighting = json!({"id":"7","tag":"GUARD","r":"nw_guard","name":"Gate Guard","combat":true,"target":"9"});
        let chat = json!([{"seq":1,"kind":"chat","id":"7","name":"Gate Guard","area":"gate","channel":1,"text":"Halt!"}]);
        live.sync(report(2, json!([fighting]), chat).to_string().as_bytes(), now_ms()).unwrap();
        live.server_log(&json!({"v":1,"kind":"server_log","stream":"s","seq":1,"line":"Script error: x","script_error":true}), now_ms());
        let all = call("live_events", json!({}), &live, &stop).unwrap();
        let kinds: Vec<&str> =
            all["events"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["chat", "combat_start", "target", "server_log"]);
        assert_eq!(all["last"], 4);
        let newer = call("live_events", json!({"since":3}), &live, &stop).unwrap();
        assert_eq!(newer["events"][0]["data"]["script_error"], true);
        let said = call(
            "live_events",
            json!({"area":"gate","kinds":["chat"],"text":"halt"}),
            &live,
            &stop,
        )
        .unwrap();
        assert_eq!(said["events"][0]["data"]["text"], "Halt!");
        assert!(call("live_events", json!({"limit":501}), &live, &stop).is_err());
        let found = call("live_find", json!({"text":"guard"}), &live, &stop).unwrap();
        assert_eq!(found["objects"][0]["area"], "gate");
        assert_eq!(
            (found["objects"][0]["id"].as_str(), found["objects"][0]["combat"].as_bool()),
            (Some("7"), Some(true))
        );
        assert!(
            call("live_find", json!({"text":"nobody"}), &live, &stop).unwrap()["objects"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        // The game's answer to a command is an event too.
        call(
            "live_npc_say",
            json!({"area":"gate","target":"7","text":"Go","id":"say-1"}),
            &live,
            &stop,
        )
        .unwrap();
        let answer = json!({"v":1,"boot":"ev","seq":3,"results":[{"id":"say-1","ok":false,"err":"target is dead","code":"dead"}]});
        live.sync(answer.to_string().as_bytes(), now_ms()).unwrap();
        let commands = call("live_events", json!({"kinds":["command"]}), &live, &stop).unwrap();
        assert_eq!(commands["events"][0]["data"]["code"], "dead");
    }

    #[test]
    fn visual_effects_have_transforms_but_no_colour_input() {
        let all = table(Policy::default());
        let op = all.iter().find(|op| op.descriptor["name"] == "live_apply_vfx").unwrap();
        let input = json!({"area":"events","target":"7","row":2,"duration":"permanent",
            "scale":1.5,"offset":{"x":1,"y":2,"z":3},"rotation":{"x":10,"y":20,"z":30}});
        validate(&op.descriptor["input"], &input).unwrap();
        let mut obsolete = input;
        obsolete["colour"] = json!([1, 0, 0, 1]);
        assert!(validate(&op.descriptor["input"], &obsolete).is_err());
    }

    #[test]
    fn retired_shader_operations_are_not_discoverable_or_dispatchable() {
        let catalogue = table(Policy { players: true, scripts: true, chunks: true });
        for name in ["set_material_uniform", "set_player_shader_uniform", "replace_object_texture"]
        {
            assert!(catalogue.iter().all(|op| op.descriptor["name"] != format!("live_{name}")));
            assert!(serde_json::from_value::<crate::store::Op>(json!({"op":name})).is_err());
            assert_eq!(crate::access::group(name), None);
        }
    }

    #[test]
    fn openapi_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/live-api.openapi.json");
        let generated = serde_json::to_string_pretty(&openapi(Policy {
            scripts: true,
            chunks: true,
            players: true,
        }))
        .unwrap()
            + "\n";
        if std::env::var_os("MOONGLOW_UPDATE_OPENAPI").is_some() {
            std::fs::write(&path, &generated).unwrap();
        }
        assert!(
            std::fs::read_to_string(path).unwrap().replace("\r\n", "\n") == generated,
            "Regenerate with MOONGLOW_UPDATE_OPENAPI=1 cargo test -p mg-live openapi"
        );
    }
}
