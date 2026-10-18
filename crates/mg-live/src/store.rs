//! Live mode: what a running game reports about its areas and the commands
//! queued for it (protocol `mg-live.v1`). Redis carries reports and commands;
//! the game acknowledges commands in its next report.

#[path = "assistant.rs"]
pub mod assistant;
#[cfg(feature = "measure")]
#[path = "measure_store.rs"]
pub mod measure;

use glam::Vec3;
use mg_core::ObjectKind;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[path = "atmosphere.rs"]
mod atmosphere;
#[path = "store_delta.rs"]
mod delta;
#[path = "store_events.rs"]
pub(crate) mod event_store;
#[path = "store_gateway.rs"]
pub mod gateway;
#[cfg(test)]
#[path = "measure.rs"]
mod measurement;
#[path = "store_mirror.rs"]
pub mod mirror;
#[path = "store_report.rs"]
mod report;
#[path = "store_watch.rs"]
mod watch;
pub use atmosphere::{AudioAction, DayNight, DayNightBoth, MusicSlot, Weather};
use delta::DeltaPlan;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Script capabilities chosen by the operator. Chunks also need server consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    pub scripts: bool,
    pub chunks: bool,
    /// Local consent; the game independently requires MGLIVE_ALLOW_PLAYER_OPS=1.
    pub players: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self { scripts: true, chunks: false, players: false }
    }
}

/// A command not acknowledged within this long expires.
pub const COMMAND_TTL_MS: u64 = 30_000;
const MAX_QUEUE: usize = 256;
const MAX_RESULTS: usize = 64;
const MAX_AREAS: usize = 64;
/// A report carries at most 256 (the game's buffer); this leaves room.
const MAX_EVENTS: usize = 1024;
const MAX_OBJECTS: usize = 4096;
const MAX_TEXT: usize = 256;
/// How far (metres, in the plane) a reported object may lie from its GIT
/// instance and still be taken for it.
pub const MATCH_DISTANCE: f32 = 0.1;

/// A faction actually loaded by this server; IDs are repute.fac list indices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerFaction {
    pub id: u32,
    pub name: String,
}

/// One object as the game reports it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveObject {
    pub id: String,
    pub tag: String,
    #[serde(rename = "r")]
    pub resref: String,
    pub name: String,
    /// Native portrait base resref; append a portrait size suffix when resolving.
    pub portrait: Option<String>,
    pub pc: bool,
    pub dm: bool,
    /// DM possession is protected even when the creature is not itself a DM.
    pub possessed: bool,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Facing, degrees anticlockwise from east.
    #[serde(rename = "f")]
    pub facing: f32,
    pub hp: Option<[i32; 2]>,
    /// `GetCurrentAction`.
    pub act: i32,
    pub dead: bool,
    pub combat: bool,
    pub target: Option<String>,
    pub hostile: Vec<String>,
    /// PC IDs that regard this creature as friendly; absent in older reports.
    pub friendly: Option<Vec<String>>,
    /// Module faction index from NWNX_Creature_GetFaction.
    pub faction: Option<u32>,
    pub look: Option<Value>,
    #[serde(rename = "lookHash")]
    pub look_hash: Option<i32>,
    pub open: Option<bool>,
    pub lock: Option<bool>,
    pub vfx: Vec<i32>,
    /// Runtime row: appearance.2da (creature), placeables.2da or genericdoors.2da.
    pub app: Option<u16>,
    /// Appearance when first reported in this module run; absent on older servers.
    pub app_original: Option<u16>,
    pub frozen: bool,
    pub gold: Option<i32>,
    pub loot: Option<bool>,
    pub xp: Option<i32>,
    /// Actual class levels from GetHitDice, not XP eligibility.
    pub level: Option<u8>,
    pub vt: Option<VisualTransform>,
    pub useable: Option<bool>,
    #[serde(rename = "static")]
    pub is_static: Option<bool>,
    /// Native animation state, not a NWScript ANIMATION_* command constant.
    pub anim: Option<i32>,
    /// Current Moonglow animation request; native pose is separately reported in anim.
    pub animation: Option<Value>,
    /// Made by a live command.
    pub own: bool,
}

impl LiveObject {
    pub fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct AreaReport {
    area: String,
    size: Option<[u32; 2]>,
    full: bool,
    cre: Vec<LiveObject>,
    plc: Vec<LiveObject>,
    doors: Vec<LiveObject>,
    items: Vec<LiveObject>,
    waypoints: Vec<LiveObject>,
    sounds: Vec<LiveObject>,
    encounters: Vec<LiveObject>,
    stores: Vec<LiveObject>,
    triggers: Vec<LiveObject>,

    removed: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SyncRequest {
    v: u32,
    boot: String,
    #[serde(default)]
    seq: u64,
    #[serde(default)]
    base_seq: Option<u64>,
    #[serde(default)]
    factions: Option<Vec<ServerFaction>>,
    #[serde(default)]
    watched_areas: Option<Vec<String>>,
    #[serde(default)]
    ack: Vec<String>,
    #[serde(default)]
    results: Vec<CommandResult>,
    #[serde(default)]
    areas: Vec<AreaReport>,
    /// The game's events since its last report (`docs/live-events-wire.md`).
    #[serde(default)]
    events: Vec<Value>,
    /// New producers route game events exclusively through the durable stream.
    #[serde(default)]
    event_stream: bool,
    #[serde(default)]
    dropped: i64,
}

/// Stable refusal reasons shared by the game, UI and client API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    OtherArea,
    Dead,
    Protected,
    Invalid,
    Disabled,
    Forbidden,
    /// Execution was acknowledged, but its outcome is not known.
    Unknown,
    Ambiguous,
    UnknownScript,
    CompileError,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    pub code: ErrorCode,
    pub error: String,
}
impl CommandError {
    pub fn new(code: ErrorCode, error: impl Into<String>) -> Self {
        Self { code, error: error.into() }
    }
}
impl From<String> for CommandError {
    fn from(error: String) -> Self {
        Self::new(ErrorCode::Invalid, error)
    }
}
impl From<&str> for CommandError {
    fn from(error: &str) -> Self {
        error.to_owned().into()
    }
}
impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for CommandError {}
impl std::ops::Deref for CommandError {
    type Target = str;
    fn deref(&self) -> &str {
        &self.error
    }
}

/// What the game did with a command.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandResult {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    pub ok: bool,
    /// Opaque game-issued version; receipts must not acknowledge a newer result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<String>,
    /// Accepted interactive command; more results follow under the same id.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
    pub err: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
    /// The object it made, if any.
    pub obj: String,
    /// A temporary faction change made to permit same-faction combat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faction: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub affected: Vec<String>,
    /// Present only for return-enabled chunks; Some(Null) is an explicit JSON null.
    #[serde(default, deserialize_with = "present_value", skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// Bounded ObjectToJson snapshot (separate from 16 KB ordinary returns).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_json: Option<Value>,
}

fn present_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformVector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl TransformVector {
    fn validate(&self, limit: f32) -> Result<(), String> {
        finite(&[self.x, self.y, self.z])?;
        if [self.x, self.y, self.z].iter().any(|n| n.abs() > limit) {
            return Err(format!("Transform components must be within -{limit}..{limit}"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualTransform {
    pub scale: f32,
    pub rotate: TransformVector,
    pub translate: TransformVector,
}

impl Default for VisualTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotate: TransformVector::default(),
            translate: TransformVector::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardFaction {
    Hostile,
    Commoner,
    Merchant,
    Defender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Faction {
    Name(StandardFaction),
    Id(u32),
}

impl Faction {
    fn validate(&self) -> Result<(), String> {
        if matches!(self, Self::Id(n) if *n > 65535) {
            Err("Faction id must be 0-65535".into())
        } else {
            Ok(())
        }
    }
}

/// NWScript ANIMATION_* numeric value, or a model animation name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CreatureAnimation {
    Id(i32),
    Name(String),
}
impl CreatureAnimation {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Id(n) if (0..=92).contains(n) || (100..=116).contains(n) => Ok(()),
            Self::Id(_) => Err("Expected a creature ANIMATION_* constant (0-92 or 100-116)".into()),
            Self::Name(name) => resref(name),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechVolume {
    #[default]
    Talk,
    Whisper,
    Shout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Animation {
    Activate,
    Deactivate,
    Open,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnKind {
    Creature,
    Placeable,
    Item,
    Waypoint,
    Sound,
    Encounter,
    Store,
    Trigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerShape {
    Circle,
    Rectangle,
}

/// What a command asks the game to do. The game refuses all but spawning
/// for player characters and DMs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    WatchAreas {
        areas: Vec<String>,
        on: bool,
    },
    SpawnTrigger {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r: Option<String>,
        x: f32,
        y: f32,
        z: f32,
        f: f32,
        shape: TriggerShape,
        radius: Option<f32>,
        width: Option<f32>,
        height: Option<f32>,
    },
    GiveItem {
        target: String,
        resref: String,
    },
    EquipItem {
        target: String,
        item: String,
        slot: u8,
    },
    UnequipItem {
        target: String,
        item: String,
        slot: u8,
    },
    UseItem {
        target: String,
        item: String,
        #[serde(default)]
        property: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recipient: Option<String>,
    },
    TransferItem {
        target: String,
        item: String,
        recipient: String,
    },
    DropItem {
        target: String,
        item: String,
    },
    PlayCreatureAnimation {
        target: String,
        animation: CreatureAnimation,
        #[serde(rename = "loop", default)]
        looping: bool,
        #[serde(default = "default_animation_speed")]
        speed: f32,
        #[serde(default = "default_animation_seconds")]
        seconds: f32,
    },
    TakeGold {
        target: String,
        amount: u32,
    },
    SetXp {
        target: String,
        value: u32,
    },
    TakeXp {
        target: String,
        amount: u32,
    },
    RemoveLevel {
        target: String,
    },
    GiveLevel {
        target: String,
    },
    GiveAlignment {
        target: String,
        law_chaos: i32,
        good_evil: i32,
    },
    SetStat {
        target: String,
        ability: Ability,
        value: u8,
    },
    GetFactionReputation {
        target: String,
        faction: Faction,
    },
    SetFactionReputation {
        target: String,
        faction: Faction,
        reputation: u8,
    },
    DisableTrap {
        target: String,
    },
    SpawnTrapOnObject {
        target: String,
        trap: u16,
    },
    SetWeather {
        weather: Weather,
        #[serde(default)]
        all: bool,
    },
    SetFog {
        period: DayNightBoth,
        color: Option<u32>,
        amount: Option<u8>,
        #[serde(default)]
        fade: f32,
    },
    SetAreaLight {
        period: DayNight,
        ambient: Option<u32>,
        diffuse: Option<u32>,
        #[serde(default)]
        fade: f32,
    },
    SetSkybox {
        row: u32,
    },
    SetMusic {
        slot: MusicSlot,
        track: Option<u32>,
        action: Option<AudioAction>,
    },
    SetAmbientSound {
        period: DayNight,
        track: Option<u32>,
        volume: Option<u8>,
        action: Option<AudioAction>,
    },
    SoundAt {
        resref: String,
        x: f32,
        y: f32,
        z: f32,
    },
    SetTime {
        hour: u8,
        minute: u8,
    },
    SetDate {
        year: u16,
        month: u8,
        day: u8,
    },
    JumpAllPlayers {
        x: f32,
        y: f32,
        z: f32,
    },
    ViewInventory {
        target: String,
    },
    TakeItem {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resref: Option<String>,
    },
    GetVariable {
        target: String,
        #[serde(rename = "type")]
        kind: LocalType,
        name: String,
    },
    SetVariable {
        target: String,
        #[serde(rename = "type")]
        kind: LocalType,
        name: String,
        value: Value,
    },
    DumpLocals {
        target: String,
    },
    Spawn {
        kind: SpawnKind,
        r: String,
        x: f32,
        y: f32,
        z: f32,
        f: f32,
        #[serde(default = "one")]
        count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        faction: Option<Faction>,
    },
    Destroy {
        target: String,
    },
    NpcMove {
        target: String,
        x: f32,
        y: f32,
        z: f32,
        #[serde(default)]
        run: bool,
        #[serde(default)]
        hold: bool,
        #[serde(default = "hold_timeout")]
        timeout: u32,
    },
    NpcSay {
        target: String,
        text: String,
        #[serde(default)]
        volume: SpeechVolume,
    },
    ReplyToPlayer {
        player: String,
        text: String,
        #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
        speaker: Option<String>,
    },
    NpcAttack {
        target: String,
        victim: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        faction: Option<Faction>,
        #[serde(default)]
        hold: bool,
        #[serde(default = "hold_timeout")]
        timeout: u32,
    },
    SetState {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        open: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lock: Option<bool>,
    },
    StopCombat {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<f32>,
    },
    SetFaction {
        target: String,
        faction: Faction,
    },
    Freeze {
        target: String,
        seconds: f32,
    },
    Unfreeze {
        target: String,
    },
    ApplyVfx {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<TransformVector>,
        row: u16,
        duration: VfxDuration,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seconds: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offset: Option<TransformVector>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rotation: Option<TransformVector>,
    },
    SetTile {
        x: u16,
        y: u16,
        tile_id: u16,
        orientation: u8,
        height: u8,
    },
    Patrol {
        target: String,
        points: Vec<TransformVector>,
        #[serde(default)]
        r#loop: bool,
        #[serde(default)]
        run: bool,
    },
    StopPatrol {
        target: String,
    },
    SkillCheck {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<f32>,
        skill: u16,
        dc: u8,
        #[serde(default)]
        bonus: i16,
        #[serde(default = "yes")]
        show_dc: bool,
        #[serde(default = "skill_timeout")]
        timeout: u16,
        #[serde(default)]
        success_text: String,
        #[serde(default)]
        fail_text: String,
    },
    CaptureObject {
        target: String,
    },
    SpawnJson {
        json: Value,
        x: f32,
        y: f32,
        z: f32,
        f: f32,
    },

    ReplaceObjectAnimation {
        target: String,
        old: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new: Option<String>,
        #[serde(default)]
        reset: bool,
    },

    SetStatic {
        target: String,
        r#static: bool,
    },
    SetLootable {
        target: String,
        lootable: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        no_drop: Option<bool>,
    },
    SetAppearance {
        target: String,
        row: u16,
    },
    ClearVfx {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row: Option<u16>,
    },
    Kill {
        target: String,
    },
    Heal {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount: Option<u32>,
    },
    SetInvulnerable {
        target: String,
        enabled: bool,
    },
    SetImmortal {
        target: String,
        enabled: bool,
    },
    SetAi {
        target: String,
        level: AiLevel,
    },
    ForceRest {
        target: String,
    },
    Limbo {
        target: String,
    },
    Unlimbo {
        target: String,
        x: f32,
        y: f32,
        z: f32,
    },
    PcMessage {
        target: String,
        text: String,
        mode: MessageMode,
    },
    PcJournal {
        target: String,
        quest: String,
        state: u32,
    },
    PcTakeItem {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resref: Option<String>,
    },
    PcBoot {
        target: String,
        #[serde(default)]
        reason: String,
    },
    Fade {
        target: String,
        direction: FadeDirection,
        seconds: f32,
    },
    PlaySound {
        target: String,
        resref: String,
    },
    PcHeal {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount: Option<u32>,
    },
    PcRaise {
        target: String,
    },
    Teleport {
        target: String,
        x: f32,
        y: f32,
        z: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        f: Option<f32>,
    },
    PcTeleport {
        target: String,
        x: f32,
        y: f32,
        z: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        f: Option<f32>,
    },
    PcGiveGold {
        target: String,
        amount: u32,
    },
    PcGiveXp {
        target: String,
        amount: u32,
    },
    PcGiveItem {
        target: String,
        resref: String,
    },
    SetFacing {
        target: String,
        f: f32,
    },
    SetVisualTransform {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rotate: Option<TransformVector>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        translate: Option<TransformVector>,
        #[serde(default)]
        reset: bool,
    },
    SetPosition {
        target: String,
        x: f32,
        y: f32,
        z: f32,
    },
    PlayAnimation {
        target: String,
        name: Animation,
    },
    SetUseable {
        target: String,
        useable: bool,
    },
    SetName {
        target: String,
        name: String,
    },
    RunScript {
        script: String,
        #[serde(default)]
        target: Option<String>,
    },
    RunChunk {
        code: String,
        #[serde(default)]
        target: Option<String>,
        #[serde(default = "yes")]
        wrap: bool,
        #[serde(default, rename = "return", skip_serializing_if = "std::ops::Not::not")]
        return_value: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalType {
    Int,
    Float,
    String,
    Object,
    Location,
}
fn local_target(s: &str) -> Result<(), String> {
    if matches!(s, "module" | "area") { Ok(()) } else { object_id("target", s) }
}
fn local_name(s: &str) -> Result<(), String> {
    if s.is_empty()
        || s.len() > 128
        || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_. -".contains(&b))
    {
        return Err("Local name must be 1-128 ASCII letters, digits, spaces or _.-".into());
    }
    if s.to_ascii_uppercase().starts_with("MGLIVE_") || s.to_ascii_uppercase().starts_with("NWNX_")
    {
        return Err("Reserved local name".into());
    }
    Ok(())
}
impl LocalType {
    fn validate(&self, v: &Value) -> Result<(), String> {
        match self {
            Self::Int if v.as_i64().is_some_and(|n| i32::try_from(n).is_ok()) => Ok(()),
            Self::Float if v.as_f64().is_some_and(|n| (n as f32).is_finite()) => Ok(()),
            Self::String
                if v.as_str().is_some_and(|s| s.chars().count() <= 4096 && !s.contains('\0')) =>
            {
                Ok(())
            }
            Self::Object if v.is_null() => Ok(()),
            Self::Object if v.is_string() => object_id("value", v.as_str().unwrap()),
            Self::Location if v.is_null() => Ok(()),
            Self::Location => {
                let at: LocalLocation =
                    serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
                resref(&at.area)?;
                finite(&[at.x, at.y, at.z, at.f])
            }
            _ => Err("Value does not match local type or range".into()),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalLocation {
    area: String,
    x: f32,
    y: f32,
    z: f32,
    f: f32,
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
}

fn skill_timeout() -> u16 {
    120
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageMode {
    Chat,
    /// Targeted native server feedback, visibly prefixed DM; not an avatar tell.
    Dm,
    Floating,
    Nui,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FadeDirection {
    ToBlack,
    FromBlack,
}

impl Op {
    pub fn is_player(&self) -> bool {
        matches!(
            self,
            Self::ReplyToPlayer { .. }
                | Self::PcMessage { .. }
                | Self::PcJournal { .. }
                | Self::PcTakeItem { .. }
                | Self::PcBoot { .. }
                | Self::Fade { .. }
                | Self::PlaySound { .. }
                | Self::SkillCheck { .. }
                | Self::GiveLevel { .. }
                | Self::GiveAlignment { .. }
                | Self::SetStat { .. }
                | Self::JumpAllPlayers { .. }
                | Self::PcHeal { .. }
                | Self::PcRaise { .. }
                | Self::PcTeleport { .. }
                | Self::PcGiveGold { .. }
                | Self::PcGiveXp { .. }
                | Self::PcGiveItem { .. }
        )
    }
}

fn default_animation_speed() -> f32 {
    1.0
}
fn default_animation_seconds() -> f32 {
    30.0
}

fn hold_timeout() -> u32 {
    30
}

fn validate_timeout(timeout: u32) -> Result<(), String> {
    if (1..=120).contains(&timeout) { Ok(()) } else { Err("timeout must be 1-120 seconds".into()) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ability {
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Wisdom,
    Charisma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiLevel {
    Default,
    VeryLow,
    Low,
    Normal,
    High,
    VeryHigh,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub area: String,
    #[serde(flatten)]
    pub op: Op,
}

fn short(what: &str, s: &str, max: usize) -> Result<(), String> {
    if s.chars().count() > max {
        Err(format!("{what} is longer than {max} characters"))
    } else {
        Ok(())
    }
}

fn resref(s: &str) -> Result<(), String> {
    if s.is_empty()
        || s.len() > 16
        || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!("\"{s}\" is not a resref"));
    }
    Ok(())
}

fn object_id(what: &str, s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 16 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{what} \"{s}\" is not a game object id"));
    }
    Ok(())
}

fn finite(v: &[f32]) -> Result<(), String> {
    if v.iter().all(|v| v.is_finite()) { Ok(()) } else { Err("coordinates must be finite".into()) }
}

fn amount(n: u32, max: u32) -> Result<(), String> {
    if (1..=max).contains(&n) { Ok(()) } else { Err(format!("amount must be 1-{max}")) }
}

fn game_text(what: &str, text: &str) -> Result<(), String> {
    // The supported game codepages are single-byte. Escaped JSON converts each
    // Unicode scalar to a game byte; UTF-8 byte length would reject Polish early.
    if text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) {
        return Err(format!("{what} is empty"));
    }
    short(what, text, 200)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfxDuration {
    Instant,
    Temporary,
    Permanent,
}

const MAX_OBJECT_JSON: usize = 262144;
fn validate_object_json(value: &Value) -> Result<(), String> {
    fn depth(value: &Value, level: usize) -> bool {
        if level > 32 {
            return false;
        }
        match value {
            Value::Array(a) => a.iter().all(|v| depth(v, level + 1)),
            Value::Object(o) => o.values().all(|v| depth(v, level + 1)),
            Value::Number(n) => n.as_f64().is_some_and(|n| n.is_finite() && n.abs() <= 1e38),
            _ => true,
        }
    }
    if !depth(value, 0) || crate::bridge::game_json(value).len() > MAX_OBJECT_JSON {
        return Err("Object JSON exceeds 256 KiB, 32 levels or finite numeric range".into());
    }
    if !matches!(value["__data_type"].as_str(), Some("UTC " | "UTP " | "UTD " | "UTI ")) {
        return Err("Object JSON must be UTC, UTP, UTD or UTI".into());
    }
    for key in ["IsPC", "IsDM"] {
        if value.get(key).is_some_and(|v| v["value"].as_i64() != Some(0)) {
            return Err("Player/DM object JSON is not spawnable".into());
        }
    }
    // Parse with the existing checked GFF decoder, discarding the parsed tree.
    // UTF-8 is validation only: publication performs the existing game encoding.
    mg_gff::from_json(value, mg_core::Codepage::for_label("utf-8").unwrap())
        .map_err(|e| format!("Invalid object GFF JSON: {e}"))?;
    Ok(())
}

impl Command {
    /// Checks it before it is queued; the game checks it again.
    pub fn validate(&self) -> Result<(), String> {
        short("id", &self.id, 64)?;
        if !self.area.is_empty()
            || !matches!(
                self.op,
                Op::WatchAreas { .. }
                    | Op::ReplyToPlayer { .. }
                    | Op::Teleport { .. }
                    | Op::PcTeleport { .. }
            )
        {
            resref(&self.area)?;
        }
        match &self.op {
            Op::WatchAreas { areas, .. } => watch::validate(areas),
            Op::EquipItem { target, item, slot } | Op::UnequipItem { target, item, slot } => {
                object_id("target", target)?;
                object_id("item", item)?;
                if *slot <= 13 {
                    Ok(())
                } else {
                    Err("slot must be 0-13 (ordinary equipment)".into())
                }
            }
            Op::UseItem { target, item, recipient, .. } => {
                object_id("target", target)?;
                object_id("item", item)?;
                if let Some(recipient) = recipient {
                    object_id("recipient", recipient)?;
                }
                Ok(())
            }
            Op::TransferItem { target, item, recipient } => {
                object_id("target", target)?;
                object_id("item", item)?;
                object_id("recipient", recipient)?;
                if recipient == target {
                    return Err("Recipient already holds item".into());
                }
                Ok(())
            }
            Op::DropItem { target, item } => {
                object_id("target", target)?;
                object_id("item", item)
            }
            Op::PlayCreatureAnimation { target, animation, speed, seconds, .. } => {
                object_id("target", target)?;
                animation.validate()?;
                finite(&[*speed, *seconds])?;
                if (0.1..=10.).contains(speed) && (0.1..=300.).contains(seconds) {
                    Ok(())
                } else {
                    Err("speed must be 0.1-10, seconds 0.1-300".into())
                }
            }
            Op::TakeGold { target, amount: value } => {
                object_id("target", target)?;
                amount(*value, 1_000_000)
            }
            Op::TakeXp { target, amount: value } => {
                object_id("target", target)?;
                amount(*value, 100_000)
            }
            Op::SetXp { target, value } => {
                object_id("target", target)?;
                if *value <= i32::MAX as u32 {
                    Ok(())
                } else {
                    Err("XP must be 0-2147483647".into())
                }
            }
            Op::RemoveLevel { target } | Op::GiveLevel { target } | Op::SetStat { target, .. } => {
                object_id("target", target)
            }
            Op::GiveAlignment { target, law_chaos, good_evil } => {
                object_id("target", target)?;
                if (-100..=100).contains(law_chaos) && (-100..=100).contains(good_evil) {
                    Ok(())
                } else {
                    Err("Alignment deltas must be -100..100".into())
                }
            }
            Op::GetFactionReputation { target, faction } => {
                object_id("target", target)?;
                faction.validate()
            }
            Op::SetFactionReputation { target, faction, reputation } => {
                object_id("target", target)?;
                faction.validate()?;
                if *reputation <= 100 { Ok(()) } else { Err("Reputation must be 0..100".into()) }
            }
            Op::SetWeather { .. }
            | Op::SetFog { .. }
            | Op::SetAreaLight { .. }
            | Op::SetSkybox { .. }
            | Op::SetMusic { .. }
            | Op::SetAmbientSound { .. }
            | Op::SoundAt { .. } => atmosphere::validate(&self.op),
            Op::SetTime { hour, minute } => {
                if *hour <= 23 && *minute <= 59 {
                    Ok(())
                } else {
                    Err("Invalid hour/minute".into())
                }
            }
            Op::SetDate { year, month, day } => {
                if *year <= 32000 && (1..=12).contains(month) && (1..=28).contains(day) {
                    Ok(())
                } else {
                    Err("Invalid NWN calendar date".into())
                }
            }
            Op::JumpAllPlayers { x, y, z } => finite(&[*x, *y, *z]),
            Op::ViewInventory { target } => object_id("target", target),
            Op::TakeItem { target, item, resref: r }
            | Op::PcTakeItem { target, item, resref: r } => {
                object_id("target", target)?;
                match (item, r) {
                    (Some(id), None) => object_id("item", id),
                    (None, Some(r)) => resref(r),
                    _ => Err("Supply item or resref exclusively".into()),
                }
            }
            Op::GetVariable { target, kind: _, name } => {
                local_target(target)?;
                local_name(name)
            }
            Op::SetVariable { target, kind, name, value } => {
                local_target(target)?;
                local_name(name)?;
                kind.validate(value)
            }
            Op::DumpLocals { target } => local_target(target),
            Op::SpawnTrigger { r, x, y, z, f, shape, radius, width, height } => {
                if let Some(r) = r {
                    resref(r)?;
                }
                finite(&[*x, *y, *z, *f])?;
                let values = match (shape, radius, width, height) {
                    (TriggerShape::Circle, Some(r), None, None) => vec![*r],
                    (TriggerShape::Rectangle, None, Some(w), Some(h)) => vec![*w, *h],
                    _ => {
                        return Err(
                            "Circle requires radius only; rectangle width and height only".into()
                        );
                    }
                };
                if values.iter().any(|n| !n.is_finite() || *n <= 0.0 || *n > 30.0) {
                    return Err("Trigger dimensions must be >0 and <=30 metres".into());
                }
                Ok(())
            }
            Op::Spawn { kind, r, x, y, z, f, count, faction } => {
                resref(r)?;
                if !(1..=10).contains(count) {
                    return Err("count must be 1-10".into());
                }
                if let Some(faction) = faction {
                    if *kind != SpawnKind::Creature {
                        return Err("Only creatures have factions".into());
                    }
                    faction.validate()?;
                }
                finite(&[*x, *y, *z, *f])
            }
            Op::SetTile { orientation, .. } => {
                if *orientation > 3 {
                    return Err("orientation must be 0-3".into());
                }
                Ok(())
            }
            Op::Patrol { target, points, .. } => {
                object_id("target", target)?;
                if points.is_empty() || points.len() > 16 {
                    return Err("points must contain 1-16 xyz objects".into());
                }
                for p in points {
                    finite(&[p.x, p.y, p.z])?;
                }
                Ok(())
            }
            Op::StopPatrol { target } => object_id("target", target),
            Op::SkillCheck {
                target,
                x,
                y,
                radius,
                success_text,
                fail_text,
                bonus,
                timeout,
                ..
            } => {
                if !(-100..=100).contains(bonus) || !(1..=600).contains(timeout) {
                    return Err("bonus must be -100..100; timeout must be 1..600 seconds".into());
                }
                match (target, x, y, radius) {
                    (Some(target), None, None, None) => object_id("target", target)?,
                    (None, Some(x), Some(y), Some(radius)) => {
                        finite(&[*x, *y, *radius])?;
                        if *radius <= 0.0 || *radius > 30.0 {
                            return Err("radius must be >0 and <=30".into());
                        }
                    }
                    _ => return Err("Supply target or x/y/radius exclusively".into()),
                }
                for text in [success_text, fail_text] {
                    if !text.is_empty() {
                        game_text("message", text)?;
                    }
                }
                Ok(())
            }
            Op::CaptureObject { target } => object_id("target", target),
            Op::SpawnJson { json, x, y, z, f } => {
                validate_object_json(json)?;
                finite(&[*x, *y, *z, *f])
            }
            Op::ApplyVfx {
                target, location, duration, seconds, scale, offset, rotation, ..
            } => {
                if scale.is_some_and(|s| !s.is_finite() || !(0.01..=100.).contains(&s)) {
                    return Err("VFX scale must be 0.01-100".into());
                }
                for (v, limit) in [(offset, 1000.), (rotation, 3600.)] {
                    if let Some(v) = v {
                        finite(&[v.x, v.y, v.z])?;
                        if [v.x, v.y, v.z].iter().any(|x| x.abs() > limit) {
                            return Err("VFX vector exceeds limit".into());
                        }
                    }
                }
                match (target, location) {
                    (Some(target), None) => object_id("target", target)?,
                    (None, Some(p)) => finite(&[p.x, p.y, p.z])?,
                    _ => return Err("Supply target or location exclusively".into()),
                }
                match (duration, seconds) {
                    (VfxDuration::Temporary, Some(s))
                        if s.is_finite() && *s > 0.0 && *s <= 86400.0 =>
                    {
                        Ok(())
                    }
                    (VfxDuration::Instant | VfxDuration::Permanent, None) => Ok(()),
                    _ => Err("seconds is required for temporary only, >0 and <=86400".into()),
                }
            }
            Op::ReplaceObjectAnimation { target, old, new, reset } => {
                object_id("target", target)?;
                resref(old)?;
                match (reset, new) {
                    (true, None) => Ok(()),
                    (false, Some(new)) => resref(new),
                    _ => Err("Supply new or reset exclusively".into()),
                }
            }
            Op::SetStatic { target, .. }
            | Op::SetLootable { target, .. }
            | Op::SetAppearance { target, .. }
            | Op::ClearVfx { target, .. } => object_id("target", target),
            Op::DisableTrap { target }
            | Op::SpawnTrapOnObject { target, .. }
            | Op::Kill { target }
            | Op::SetInvulnerable { target, .. }
            | Op::SetImmortal { target, .. }
            | Op::SetAi { target, .. }
            | Op::ForceRest { target }
            | Op::Limbo { target } => object_id("target", target),
            Op::Unlimbo { target, x, y, z } => {
                object_id("target", target)?;
                finite(&[*x, *y, *z])
            }
            Op::Destroy { target } => object_id("target", target),
            Op::NpcMove { target, x, y, z, timeout, .. } => {
                object_id("target", target)?;
                validate_timeout(*timeout)?;
                finite(&[*x, *y, *z])
            }
            Op::ReplyToPlayer { player, text, speaker } => {
                object_id("player", player)?;
                if let Some(speaker) = speaker {
                    object_id("as", speaker)?;
                }
                if !self.area.is_empty() {
                    return Err("reply_to_player resolves the player's area; omit area".into());
                }
                game_text("text", text)
            }
            Op::PcMessage { target, text, .. } | Op::NpcSay { target, text, .. } => {
                object_id("target", target)?;
                game_text("text", text)
            }
            Op::NpcAttack { target, victim, timeout, faction, .. } => {
                if let Some(faction) = faction {
                    faction.validate()?;
                    if matches!(faction, Faction::Id(0)) {
                        return Err("Fighting faction cannot be PC".into());
                    }
                }
                object_id("target", target)?;
                object_id("victim", victim)?;
                validate_timeout(*timeout)
            }
            Op::SetState { target, open, lock } => {
                object_id("target", target)?;
                if open.is_none() && lock.is_none() {
                    return Err("set_state needs open or lock".into());
                }
                Ok(())
            }
            Op::StopCombat { target, x, y, radius } => match (target, x, y, radius) {
                (Some(target), None, None, None) => object_id("target", target),
                (None, Some(x), Some(y), Some(radius)) => {
                    finite(&[*x, *y, *radius])?;
                    if *radius <= 0.0 || *radius > 50.0 {
                        return Err("radius must be >0 and <=50".into());
                    }
                    Ok(())
                }
                _ => {
                    Err("stop_combat needs either target or all of x, y, radius, never both".into())
                }
            },
            Op::SetFaction { target, faction } => {
                object_id("target", target)?;
                faction.validate()
            }
            Op::Freeze { target, seconds } => {
                object_id("target", target)?;
                finite(&[*seconds])?;
                if *seconds <= 0.0 || *seconds > 300.0 {
                    return Err("seconds must be >0 and <=300".into());
                }
                Ok(())
            }
            Op::Unfreeze { target }
            | Op::PcRaise { target }
            | Op::SetUseable { target, .. }
            | Op::PlayAnimation { target, .. } => object_id("target", target),
            Op::Heal { target, amount: value } | Op::PcHeal { target, amount: value } => {
                object_id("target", target)?;
                value.map_or(Ok(()), |n| amount(n, 100000))
            }
            Op::PcJournal { target, quest, state } => {
                object_id("target", target)?;
                if quest.is_empty()
                    || quest.len() > 64
                    || !quest.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                    || *state > i32::MAX as u32
                {
                    return Err(
                        "quest is 1-64 ASCII letters/digits/_/-; state 0..2147483647".into()
                    );
                }
                Ok(())
            }
            Op::PcBoot { target, reason } => {
                object_id("target", target)?;
                if reason.is_empty() { Ok(()) } else { game_text("reason", reason) }
            }
            Op::Fade { target, seconds, .. } => {
                object_id("target", target)?;
                finite(&[*seconds])?;
                if *seconds > 0. && *seconds <= 60. {
                    Ok(())
                } else {
                    Err("seconds must be >0 and <=60".into())
                }
            }
            Op::PlaySound { target, resref: r } => {
                object_id("target", target)?;
                resref(r)
            }
            Op::PcGiveGold { target, amount: value } => {
                object_id("target", target)?;
                amount(*value, 1000000)
            }
            Op::PcGiveXp { target, amount: value } => {
                object_id("target", target)?;
                amount(*value, 100000)
            }
            Op::GiveItem { target, resref: r } | Op::PcGiveItem { target, resref: r } => {
                object_id("target", target)?;
                resref(r)
            }
            Op::Teleport { target, x, y, z, f } | Op::PcTeleport { target, x, y, z, f } => {
                object_id("target", target)?;
                finite(&[*x, *y, *z])?;
                if *x < 0.0 || *y < 0.0 {
                    return Err("xy must be nonnegative".into());
                }
                if f.is_some_and(|f| !f.is_finite() || !(0.0..360.0).contains(&f)) {
                    return Err("f must be finite, >=0 and <360".into());
                }
                Ok(())
            }
            Op::SetPosition { target, x, y, z } => {
                object_id("target", target)?;
                finite(&[*x, *y, *z])
            }
            Op::SetFacing { target, f } => {
                object_id("target", target)?;
                finite(&[*f])?;
                if !(0.0..360.0).contains(f) {
                    return Err("f must be >=0 and <360".into());
                }
                Ok(())
            }
            Op::SetVisualTransform { target, scale, rotate, translate, reset } => {
                object_id("target", target)?;
                let values = scale.is_some() || rotate.is_some() || translate.is_some();
                if *reset == values {
                    return Err("Supply reset:true or transform values, exclusively".into());
                }
                if let Some(scale) = scale {
                    finite(&[*scale])?;
                    if *scale <= 0.0 || *scale > 10.0 {
                        return Err("scale must be >0 and <=10".into());
                    }
                }
                if let Some(rotate) = rotate {
                    rotate.validate(360.0)?;
                }
                if let Some(translate) = translate {
                    translate.validate(1000.0)?;
                }
                Ok(())
            }
            Op::SetName { target, name } => {
                object_id("target", target)?;
                game_text("name", name)
            }
            Op::RunScript { script, target } => {
                if script.is_empty()
                    || script.len() > 16
                    || !script.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err("script must be a 1-16 character script resref".into());
                }
                target.as_deref().map_or(Ok(()), |id| object_id("target", id))
            }
            Op::RunChunk { code, target, .. } => {
                if code.trim().is_empty() {
                    return Err("code is empty".into());
                }
                short("code", code, 8000)?;
                target.as_deref().map_or(Ok(()), |id| object_id("target", id))
            }
        }
    }

    /// The object it acts on, if any.
    pub fn target(&self) -> Option<&str> {
        match &self.op {
            Op::GetVariable { target, .. }
            | Op::SetVariable { target, .. }
            | Op::DumpLocals { target } => {
                (!matches!(target.as_str(), "module" | "area")).then_some(target)
            }
            Op::WatchAreas { .. }
            | Op::SetTile { .. }
            | Op::SpawnJson { .. }
            | Op::SetWeather { .. }
            | Op::SetFog { .. }
            | Op::SetAreaLight { .. }
            | Op::SetSkybox { .. }
            | Op::SetMusic { .. }
            | Op::SetAmbientSound { .. }
            | Op::SoundAt { .. }
            | Op::SetTime { .. }
            | Op::SetDate { .. }
            | Op::JumpAllPlayers { .. }
            | Op::SpawnTrigger { .. }
            | Op::Spawn { .. } => None,
            Op::StopCombat { target, .. }
            | Op::SkillCheck { target, .. }
            | Op::ApplyVfx { target, .. }
            | Op::RunScript { target, .. }
            | Op::RunChunk { target, .. } => target.as_deref(),
            Op::GiveItem { target, .. }
            | Op::EquipItem { target, .. }
            | Op::UnequipItem { target, .. }
            | Op::DropItem { target, .. }
            | Op::UseItem { target, .. }
            | Op::TransferItem { target, .. }
            | Op::PlayCreatureAnimation { target, .. }
            | Op::Patrol { target, .. }
            | Op::StopPatrol { target }
            | Op::CaptureObject { target }
            | Op::ReplaceObjectAnimation { target, .. }
            | Op::SetStatic { target, .. }
            | Op::SetLootable { target, .. }
            | Op::SetAppearance { target, .. }
            | Op::ClearVfx { target, .. }
            | Op::TakeGold { target, .. }
            | Op::SetXp { target, .. }
            | Op::TakeXp { target, .. }
            | Op::RemoveLevel { target }
            | Op::GiveLevel { target }
            | Op::GiveAlignment { target, .. }
            | Op::SetStat { target, .. }
            | Op::GetFactionReputation { target, .. }
            | Op::SetFactionReputation { target, .. }
            | Op::DisableTrap { target }
            | Op::SpawnTrapOnObject { target, .. }
            | Op::ViewInventory { target }
            | Op::TakeItem { target, .. }
            | Op::Destroy { target }
            | Op::NpcMove { target, .. }
            | Op::NpcSay { target, .. }
            | Op::NpcAttack { target, .. }
            | Op::SetState { target, .. }
            | Op::SetFaction { target, .. }
            | Op::Freeze { target, .. }
            | Op::Unfreeze { target }
            | Op::ReplyToPlayer { player: target, .. }
            | Op::PcMessage { target, .. }
            | Op::PcJournal { target, .. }
            | Op::PcTakeItem { target, .. }
            | Op::PcBoot { target, .. }
            | Op::Fade { target, .. }
            | Op::PlaySound { target, .. }
            | Op::PcHeal { target, .. }
            | Op::PcRaise { target }
            | Op::Teleport { target, .. }
            | Op::PcTeleport { target, .. }
            | Op::PcGiveGold { target, .. }
            | Op::PcGiveXp { target, .. }
            | Op::PcGiveItem { target, .. }
            | Op::SetFacing { target, .. }
            | Op::SetVisualTransform { target, .. }
            | Op::SetPosition { target, .. }
            | Op::PlayAnimation { target, .. }
            | Op::SetUseable { target, .. }
            | Op::SetName { target, .. }
            | Op::Kill { target }
            | Op::Heal { target, .. }
            | Op::SetInvulnerable { target, .. }
            | Op::SetImmortal { target, .. }
            | Op::SetAi { target, .. }
            | Op::ForceRest { target }
            | Op::Limbo { target }
            | Op::Unlimbo { target, .. } => Some(target),
        }
    }
}

/// The last report of one area.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AreaLive {
    /// Native area width/height in metres; absent from older reports.
    pub size: Option<[u32; 2]>,
    pub seq: u64,
    pub received_ms: u64,
    /// When placeables, doors and items were last reported; 0: never.
    pub full_ms: u64,
    /// Separate from the timestamp: deterministic clocks may start at zero.
    pub has_full: bool,
    pub creatures: Vec<LiveObject>,
    pub placeables: Vec<LiveObject>,
    pub doors: Vec<LiveObject>,
    pub items: Vec<LiveObject>,
    pub waypoints: Vec<LiveObject>,
    pub sounds: Vec<LiveObject>,
    pub encounters: Vec<LiveObject>,
    pub stores: Vec<LiveObject>,
    pub triggers: Vec<LiveObject>,
}

impl AreaLive {
    /// Every reported object with its kind.
    pub fn all(&self) -> impl Iterator<Item = (ObjectKind, &LiveObject)> {
        fn tag(
            kind: ObjectKind,
            list: &[LiveObject],
        ) -> impl Iterator<Item = (ObjectKind, &LiveObject)> {
            list.iter().map(move |o| (kind, o))
        }
        tag(ObjectKind::Creature, &self.creatures)
            .chain(tag(ObjectKind::Placeable, &self.placeables))
            .chain(tag(ObjectKind::Door, &self.doors))
            .chain(tag(ObjectKind::Item, &self.items))
            .chain(tag(ObjectKind::Waypoint, &self.waypoints))
            .chain(tag(ObjectKind::Sound, &self.sounds))
            .chain(tag(ObjectKind::Encounter, &self.encounters))
            .chain(tag(ObjectKind::Store, &self.stores))
            .chain(tag(ObjectKind::Trigger, &self.triggers))
    }

    pub fn find(&self, id: &str) -> Option<(ObjectKind, &LiveObject)> {
        self.all().find(|(_, o)| o.id == id)
    }
}

#[derive(Debug, Clone)]
struct Queued {
    command: Command,
    sent: bool,
    players: bool,
    queued_ms: u64,
    ttl_ms: u64,
}

#[derive(Debug, Default)]
struct State {
    mirror: Option<mirror::Mirror>,
    gateway: gateway::GatewayState,
    assistant: assistant::AssistantState,
    command_ids: Option<crate::command_ids::Journal>,
    command_ids_required: bool,
    boot: String,
    server: Option<crate::bridge::RedisConfig>,
    seq: u64,
    has_baseline: bool,
    resync: Option<String>,
    resyncs: u64,
    delta: bool,
    received_ms: u64,
    areas: HashMap<String, AreaLive>,
    factions: Option<Vec<ServerFaction>>,
    watch: watch::Watches,
    queue: VecDeque<Queued>,
    results: VecDeque<(Command, CommandResult)>,
    /// Commands accepted by the game but awaiting a player click/timeout.
    skill_waiting: HashMap<String, (Command, u64)>,
    next: u64,
    /// Retain operation identities independently of the short result history.
    accepted: HashMap<String, Command>,
    owners: HashMap<String, String>,
    audit: Option<Arc<crate::access::Audit>>,
    prefix: String,
    policy: Policy,
    kick: Option<Sender<()>>,
    logs: VecDeque<String>,
    redis: Option<Arc<Mutex<crate::bridge::State>>>,
    events: crate::events::EventLog,
}

/// What the game last reported and what waits for it. Shared by the HTTPS
/// listener, its client endpoints and the area views.
#[derive(Debug, Default)]
pub struct LiveStore {
    state: Mutex<State>,
}

/// How this frame affected the state baseline. Side-channel events and command
/// receipts can still be processed for a gap or stale frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    Full,
    Delta,
    Gap,
    Stale,
}

/// The existing game reply together with the decision made under the store lock.
#[derive(Debug, Clone, PartialEq)]
pub struct Synced {
    pub reply: Value,
    pub outcome: SyncOutcome,
    /// The accepted request boot differed from the store boot, including the
    /// first report received by a new store.
    pub boot_changed: bool,
}

impl LiveStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn set_audit(&self, audit: Arc<crate::access::Audit>) {
        self.lock().audit = Some(audit);
    }

    pub(crate) fn require_command_identity(&self) {
        self.lock().command_ids_required = true;
    }

    /// Install the durable boundary before publishing the agent descriptor.
    pub(crate) fn set_command_identity(
        &self,
        data: &std::path::Path,
        server: &str,
    ) -> Result<(), String> {
        let mut s = self.lock();
        if s.mirror.is_some() {
            return Err("Mirrors do not journal command IDs".into());
        }
        let root = crate::command_ids::Journal::path(data, server);
        if s.command_ids.as_ref().is_some_and(|j| j.is_at(&root)) {
            return Ok(());
        }
        let mut journal = crate::command_ids::Journal::new(root)?;
        journal.select_boot(&s.boot)?;
        for id in s.accepted.keys() {
            if !journal.contains(id) {
                journal.reserve(id)?;
            }
        }
        s.command_ids = Some(journal);
        Ok(())
    }

    /// None means an older server has not published its loaded faction list.
    pub fn factions(&self) -> Option<Vec<ServerFaction>> {
        self.lock().factions.clone()
    }

    pub fn policy(&self) -> Policy {
        self.lock().policy
    }

    /// Reusing a store for another endpoint must not reuse its validated objects.
    pub(crate) fn bind_server(&self, config: &crate::bridge::RedisConfig) {
        let mut s = self.lock();
        if s.mirror.is_some() {
            return;
        }
        if s.server.as_ref() == Some(config) {
            return;
        }
        assistant::cancel(&mut s, "expired: server endpoint changed");
        for (_, result) in &mut s.results {
            result.receipt = None;
        }
        let mut commands: Vec<_> = s.queue.drain(..).map(|q| (q.command, q.sent)).collect();
        commands.extend(s.skill_waiting.drain().map(|(_, (command, _))| (command, true)));
        for (command, sent) in commands {
            let result = undelivered(&command.id, sent, "server endpoint changed");
            push_result(&mut s, command, result);
        }
        s.server = Some(config.clone());
        s.boot.clear();
        s.areas.clear();
        s.factions = None;
        s.received_ms = 0;
        s.seq = 0;
        s.has_baseline = false;
        s.resync = None;
        s.delta = false;
        s.watch.restart();
    }

    /// Both Options and the client API observe the same transport state.
    pub(crate) fn set_redis_status(&self, state: Arc<Mutex<crate::bridge::State>>) {
        self.lock().redis = Some(state);
    }

    /// Changing policy also cancels undelivered commands that it now disallows.
    pub fn set_policy(&self, policy: Policy) {
        let mut s = self.lock();
        s.policy = policy;
        if let Some(mirror) = &mut s.mirror {
            mirror.revision += 1;
            return;
        }
        let mut i = 0;
        while i < s.queue.len() {
            if let Some(error) = policy_error(policy, &s.queue[i].command.op) {
                let q = s.queue.remove(i).expect("in range");
                let result = failure(&q.command.id, ErrorCode::Disabled, error);
                push_result(&mut s, q.command, result);
            } else {
                i += 1;
            }
        }
    }

    pub fn set_kick(&self, kick: Sender<()>) {
        self.lock().kick = Some(kick);
    }

    /// Reported objects whose name, tag or resref has `text` (any case),
    /// in every area: at most `limit`.
    pub fn find(&self, text: &str, limit: usize) -> Vec<Value> {
        let needle = text.to_lowercase();
        let s = self.lock();
        let mut areas: Vec<_> = s.areas.iter().collect();
        areas.sort_by(|a, b| a.0.cmp(b.0));
        let mut out = Vec::new();
        for (area, live) in areas {
            let lists = [
                ("creature", &live.creatures),
                ("placeable", &live.placeables),
                ("door", &live.doors),
                ("item", &live.items),
                ("waypoint", &live.waypoints),
                ("sound", &live.sounds),
                ("encounter", &live.encounters),
                ("store", &live.stores),
                ("trigger", &live.triggers),
            ];
            for (kind, list) in lists {
                for o in list.iter().filter(|o| {
                    [&o.name, &o.tag, &o.resref].iter().any(|t| t.to_lowercase().contains(&needle))
                }) {
                    if out.len() >= limit {
                        return out;
                    }
                    out.push(json!({"area": area, "kind": kind, "id": o.id, "name": o.name,
                        "tag": o.tag, "r": o.resref, "x": o.x, "y": o.y, "z": o.z, "pc": o.pc,
                        "dead": o.dead, "combat": o.combat}));
                }
            }
        }
        out
    }

    /// A message of the server log's channel.
    pub fn server_log(&self, message: &Value, now_ms: u64) {
        if self.is_mirror() {
            return;
        }
        let mut message = message.clone();
        crate::wire_text::server_log(&mut message);
        self.lock().events.server_log(&message, now_ms);
    }

    pub fn take_logs(&self) -> Vec<String> {
        self.lock().logs.drain(..).collect()
    }

    /// The current queue, also used after a reconnect; expire before publishing.
    pub fn commands(&self, now_ms: u64) -> Value {
        let mut s = self.lock();
        expire(&mut s, now_ms);
        if let Some(mirror) = &s.mirror {
            return json!({"commands": mirror.queue});
        }
        game_commands(&mut s)
    }

    /// Takes the game's post (JSON) and returns the reply: the commands it
    /// has not yet acknowledged.
    pub fn sync(&self, body: &[u8], now_ms: u64) -> Result<Value, String> {
        self.sync_tapped(body, now_ms).map(|synced| synced.reply)
    }

    /// Applies the game's report and exposes the exact state-acceptance decision.
    /// Parsing, receipts, events, queue processing and reply semantics match [`Self::sync`].
    pub fn sync_tapped(&self, body: &[u8], now_ms: u64) -> Result<Synced, String> {
        if self.is_mirror() {
            return Err("A mirror cannot ingest game reports".into());
        }
        let request = SyncRequest::parse(body)?;
        let mut s = self.lock();
        let boot_changed = s.boot != request.boot;
        if boot_changed {
            assistant::cancel(&mut s, "expired: the server restarted before DM approval");
            // A restarted server: what we knew and queued no longer applies.
            let old: Vec<Queued> = s.queue.drain(..).collect();
            for q in old {
                let id = q.command.id.clone();
                push_result(&mut s, q.command, undelivered(&id, q.sent, "the server restarted"));
            }
            let waiting: Vec<_> = s.skill_waiting.drain().map(|(_, pair)| pair.0).collect();
            for command in waiting {
                let answer = failure(
                    &command.id,
                    ErrorCode::Unknown,
                    "unknown: the server restarted while waiting for rolls",
                );
                push_result(&mut s, command, answer);
            }
            s.areas.clear();
            s.factions = None;
            s.watch.restart();
            s.boot = request.boot.clone();
            s.events.changed.notify_all();
            s.seq = 0;
            s.has_baseline = false;
            s.resync = None;
            s.delta = false;
        }
        // A delta must extend the exact state baseline; still accept its acks/events.
        if let Some(journal) = &mut s.command_ids
            && let Err(error) = journal.select_boot(&request.boot)
        {
            s.received_ms = 0;
            return Err(error);
        }
        let fresh = request.seq > s.seq || (!s.has_baseline && request.base_seq.is_none());
        if fresh && request.factions.is_some() {
            s.factions.clone_from(&request.factions);
        }
        watch::receive(&mut s.watch, &request, fresh, now_ms);
        let gap = request.base_seq.is_some()
            && fresh
            && (!s.has_baseline
                || s.resync.is_some()
                || request.base_seq != Some(s.seq)
                || request.seq != s.seq + 1
                || request
                    .areas
                    .iter()
                    .any(|a| !a.full && !s.areas.contains_key(&a.area.to_ascii_lowercase())));
        let mut outcome = if gap { SyncOutcome::Gap } else { SyncOutcome::Stale };
        let control_ack = s.resync.as_ref().is_some_and(|id| request.ack.contains(id));
        if gap && (s.resync.is_none() || control_ack) {
            let token = crate::service::generate_token()?;
            s.resync = Some(format!("resync-{}", &token[..32]));
            s.resyncs += 1;
        }
        // Prepare once, without mutating the baseline. Every retained size must
        // pass before any acknowledgements, events or area updates are applied.
        let mut prepared = Vec::with_capacity(request.areas.len());
        for a in &request.areas {
            let merged = if !gap && fresh && request.base_seq.is_some() && !a.full {
                s.areas
                    .get(&a.area.to_ascii_lowercase())
                    .map(|old| DeltaPlan::new(old, a))
                    .transpose()?
            } else {
                None
            };
            prepared.push(merged);
        }
        let enriched = event_store::enrich(
            &s,
            if request.event_stream { &[] } else { &request.events },
            now_ms,
        );
        s.events.game(&request.boot, &enriched, request.dropped, now_ms);
        receive_results(&mut s, request.results, request.ack, request.seq, now_ms);
        expire(&mut s, now_ms);
        if request.seq >= s.seq {
            s.seq = request.seq;
            s.received_ms = now_ms;
            s.delta = request.base_seq.is_some();
            if !gap && (fresh || request.base_seq.is_none()) {
                outcome =
                    if request.base_seq.is_none() { SyncOutcome::Full } else { SyncOutcome::Delta };
                if request.base_seq.is_none() {
                    s.resync = None;
                }
                s.has_baseline = true;
                let State { areas, events, gateway, .. } = &mut *s;
                for (mut a, merged) in request.areas.into_iter().zip(prepared) {
                    *gateway.area_revs.entry(a.area.to_ascii_lowercase()).or_default() += 1;
                    let live = areas.entry(a.area.to_ascii_lowercase()).or_default();
                    let seen = live.received_ms != 0;
                    if let Some(merged) = merged {
                        events.derive_iter(
                            &a.area,
                            seen.then_some(&live.creatures[..]),
                            merged.creatures(live, &a),
                            now_ms,
                        );
                        merged.apply(live, &mut a);
                    } else {
                        let mut creatures = a.cre;
                        if !a.full {
                            for o in &mut creatures {
                                if o.look.is_none()
                                    && let Some(old) =
                                        live.creatures.iter().find(|old| old.id == o.id)
                                {
                                    o.look.clone_from(&old.look);
                                    o.look_hash = old.look_hash;
                                }
                            }
                        }
                        events.derive(
                            &a.area,
                            seen.then_some(&live.creatures[..]),
                            &creatures,
                            now_ms,
                        );
                        live.creatures = creatures;
                        if a.full {
                            live.full_ms = now_ms;
                            live.has_full = true;
                            live.placeables = a.plc;
                            live.doors = a.doors;
                            live.items = a.items;
                            live.waypoints = a.waypoints;
                            live.sounds = a.sounds;
                            live.encounters = a.encounters;
                            live.stores = a.stores;
                            live.triggers = a.triggers;
                        }
                    }
                    live.size = a.size.or(live.size);
                    live.received_ms = now_ms;
                    live.seq = request.seq;
                }
            }
        }
        assistant::capture(&mut s);
        watch::prepare(&mut s.watch, now_ms);
        Ok(Synced { reply: game_commands(&mut s), outcome, boot_changed })
    }

    /// Queues a command. A command whose id is already queued or answered
    /// is not queued again: its id is returned (idempotent retries).
    pub fn enqueue(&self, command: Command, now_ms: u64) -> Result<String, CommandError> {
        let mut scope = crate::access::Scope::default();
        scope.name = "local".into();
        self.enqueue_scoped(command, now_ms, &scope)
    }

    pub fn enqueue_scoped(
        &self,
        mut command: Command,
        now_ms: u64,
        scope: &crate::access::Scope,
    ) -> Result<String, CommandError> {
        command.validate()?;
        let operation = serde_json::to_value(&command.op).expect("operation serializes");
        if !scope.allows(operation["op"].as_str().unwrap_or_default()) {
            return Err(CommandError::new(
                ErrorCode::Forbidden,
                "Client scope does not allow this operation",
            ));
        }
        let mut s = self.lock();
        expire(&mut s, now_ms);
        let ttl_ms = if let Some((boot, ttl)) = gateway::command_binding(&s.gateway, &command.id) {
            if boot != s.boot {
                return Err(CommandError::new(
                    ErrorCode::Expired,
                    "Game boot changed before enqueue",
                ));
            }
            ttl
        } else {
            COMMAND_TTL_MS
        };
        if s.mirror.is_some() {
            return mirror::enqueue(&mut s, command, now_ms, scope);
        }
        if s.command_ids_required && s.command_ids.is_none() {
            return Err(CommandError::new(
                ErrorCode::Disabled,
                "Live command identity storage is not ready",
            ));
        }
        let speech = matches!(
            command.op,
            Op::NpcSay { .. } | Op::ReplyToPlayer { .. } | Op::PcMessage { .. }
        ) && scope.has(crate::access::ClientGroup::Speech);
        let player_scope = scope.has(crate::access::ClientGroup::Players) || speech;
        let players = s.policy.players && player_scope;
        if command.op.is_player() && !player_scope {
            return Err(CommandError::new(
                ErrorCode::Forbidden,
                "Client cannot perform player operations",
            ));
        }
        if let Some(error) = policy_error(s.policy, &command.op) {
            return Err(CommandError::new(ErrorCode::Disabled, error));
        }
        if s.boot.is_empty() {
            return Err("No game has reported yet".into());
        }
        command.area.make_ascii_lowercase();
        if let Some(previous) = s.accepted.get(&command.id) {
            if !scope.unrestricted() && s.owners.get(&command.id) != Some(&scope.name) {
                return Err(CommandError::new(
                    ErrorCode::Forbidden,
                    "Command id belongs to another client",
                ));
            }
            return if previous == &command {
                Ok(command.id)
            } else {
                Err("Command id already belongs to a different operation".into())
            };
        }
        if s.command_ids.as_ref().is_some_and(|j| j.contains(&command.id)) {
            return Err(
                "Command id was already reserved in this server boot; it will not run again".into(),
            );
        }
        let teleport = matches!(command.op, Op::Teleport { .. } | Op::PcTeleport { .. });
        // Teleport's area is the destination. Keep the original optional value
        // in the accepted command so retries remain stable after a cross-area move.
        let control_area = AreaLive::default();
        let area = if matches!(command.op, Op::WatchAreas { .. }) {
            &control_area // observation control must bootstrap an unreported area
        } else if teleport || matches!(command.op, Op::ReplyToPlayer { .. }) {
            s.areas
                .values()
                .find(|a| command.target().is_some_and(|id| a.find(id).is_some()))
                .ok_or_else(|| {
                    CommandError::new(ErrorCode::NotFound, "Target is not in the latest reports")
                })?
        } else {
            s.areas
                .get(&command.area)
                .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "No report for this area"))?
        };
        if teleport {
            let destination =
                if command.area.is_empty() { Some(area) } else { s.areas.get(&command.area) };
            let (x, y) = match command.op {
                Op::Teleport { x, y, .. } | Op::PcTeleport { x, y, .. } => (x, y),
                _ => unreachable!(),
            };
            if x < 0.0
                || y < 0.0
                || destination
                    .and_then(|a| a.size)
                    .is_some_and(|[w, h]| x >= w as f32 || y >= h as f32)
            {
                return Err("xy must be inside destination area bounds".into());
            }
            // Unreported destination areas are resolved and bounded by the game.
        }
        let recipient = match &command.op {
            Op::UseItem { recipient, .. } => recipient.as_deref(),
            Op::TransferItem { recipient, .. } => Some(recipient.as_str()),
            _ => None,
        };
        if let Some(recipient) = recipient {
            let (kind, object) = find_target(&s, area, recipient)?;
            if kind != ObjectKind::Creature {
                return Err("Recipient must be a creature".into());
            }
            if (object.pc || object.dm || object.possessed) && !players {
                return Err(CommandError::new(
                    ErrorCode::Disabled,
                    "Player recipient requires both permissions",
                ));
            }
            if object.dead {
                return Err(CommandError::new(ErrorCode::Dead, "Recipient is dead"));
            }
        }
        // A creature in module limbo has no area row; the game validates its identity/type.
        if let Some(target) = command.target().filter(|_| !matches!(command.op, Op::Unlimbo { .. }))
        {
            let (kind, object) = find_target(&s, area, target)?;
            if teleport && (object.pc || object.dm || object.possessed) && !players {
                return Err(CommandError::new(
                    ErrorCode::Disabled,
                    "Player teleport requires both player permissions",
                ));
            }
            if command.op.is_player() && kind != ObjectKind::Creature {
                return Err("Target must be a creature".into());
            }
            if (object.pc || object.dm || object.possessed)
                && !players
                && matches!(
                    command.op,
                    Op::GiveItem { .. }
                        | Op::EquipItem { .. }
                        | Op::UnequipItem { .. }
                        | Op::DropItem { .. }
                        | Op::UseItem { .. }
                        | Op::TransferItem { .. }
                        | Op::PlayCreatureAnimation { .. }
                        | Op::TakeGold { .. }
                        | Op::SetXp { .. }
                        | Op::TakeXp { .. }
                        | Op::RemoveLevel { .. }
                        | Op::GetFactionReputation { .. }
                        | Op::SetFactionReputation { .. }
                )
            {
                return Err(CommandError::new(
                    ErrorCode::Disabled,
                    "Player operations require both player permissions",
                ));
            }
            if object.dead
                && matches!(
                    command.op,
                    Op::Teleport { .. }
                        | Op::GiveItem { .. }
                        | Op::EquipItem { .. }
                        | Op::UnequipItem { .. }
                        | Op::DropItem { .. }
                        | Op::UseItem { .. }
                        | Op::TransferItem { .. }
                        | Op::PlayCreatureAnimation { .. }
                        | Op::TakeGold { .. }
                        | Op::SetXp { .. }
                        | Op::TakeXp { .. }
                        | Op::RemoveLevel { .. }
                        | Op::GiveLevel { .. }
                        | Op::GiveAlignment { .. }
                        | Op::SetStat { .. }
                        | Op::GetFactionReputation { .. }
                        | Op::SetFactionReputation { .. }
                        | Op::SetLootable { .. }
                        | Op::Kill { .. }
                        | Op::Heal { .. }
                        | Op::ForceRest { .. }
                        | Op::Limbo { .. }
                        | Op::Patrol { .. }
                        | Op::NpcMove { .. }
                        | Op::NpcAttack { .. }
                        | Op::NpcSay { .. }
                        | Op::StopCombat { .. }
                        | Op::SetFaction { .. }
                        | Op::Freeze { .. }
                        | Op::Unfreeze { .. }
                        | Op::RunScript { .. }
                        | Op::RunChunk { .. }
                )
                || object.dead
                    && command.op.is_player()
                    && !matches!(
                        command.op,
                        Op::PcRaise { .. }
                            | Op::ReplyToPlayer { .. }
                            | Op::PcMessage { .. }
                            | Op::PcJournal { .. }
                            | Op::PcTakeItem { .. }
                            | Op::PcBoot { .. }
                            | Op::Fade { .. }
                            | Op::PlaySound { .. }
                    )
            {
                return Err(CommandError::new(ErrorCode::Dead, "Target is dead"));
            }
            let player_stop = matches!(command.op, Op::StopCombat { target: Some(_), .. })
                && object.pc
                && !object.dm
                && !object.possessed;
            if player_stop && !players {
                return Err(CommandError::new(
                    ErrorCode::Disabled,
                    "Player stop requires both player permissions",
                ));
            }
            if (object.pc || object.dm || object.possessed)
                && !matches!(command.op, Op::GetVariable { .. } | Op::DumpLocals { .. })
                && !matches!(command.op, Op::RunScript { .. } | Op::RunChunk { .. })
                && !(players
                    && (command.op.is_player()
                        || player_stop
                        || matches!(
                            command.op,
                            Op::Teleport { .. }
                                | Op::NpcMove { .. }
                                | Op::NpcAttack { .. }
                                | Op::ReplaceObjectAnimation { .. }
                                | Op::SetAppearance { .. }
                                | Op::ApplyVfx { .. }
                                | Op::ClearVfx { .. }
                                | Op::ViewInventory { .. }
                                | Op::TakeItem { .. }
                                | Op::SetVariable { .. }
                                | Op::Freeze { .. }
                                | Op::Unfreeze { .. }
                                | Op::Kill { .. }
                                | Op::GiveItem { .. }
                                | Op::EquipItem { .. }
                                | Op::UnequipItem { .. }
                                | Op::DropItem { .. }
                                | Op::UseItem { .. }
                                | Op::TransferItem { .. }
                                | Op::PlayCreatureAnimation { .. }
                                | Op::TakeGold { .. }
                                | Op::SetXp { .. }
                                | Op::TakeXp { .. }
                                | Op::RemoveLevel { .. }
                                | Op::GetFactionReputation { .. }
                                | Op::SetFactionReputation { .. }
                                | Op::Heal { .. }
                                | Op::SetInvulnerable { .. }
                                | Op::SetImmortal { .. }
                                | Op::SetAi { .. }
                                | Op::ForceRest { .. }
                        )))
            {
                return Err(CommandError::new(
                    ErrorCode::Protected,
                    "Target is a player, DM or DM-possessed creature",
                ));
            }
            match &command.op {
                Op::Destroy { .. } if kind == ObjectKind::Encounter => {
                    return Err(CommandError::new(
                        ErrorCode::Disabled,
                        "Native encounter destruction is unavailable; object would remain active",
                    ));
                }
                Op::GetFactionReputation { .. }
                | Op::SetFactionReputation { .. }
                | Op::GiveItem { .. }
                | Op::EquipItem { .. }
                | Op::UnequipItem { .. }
                | Op::DropItem { .. }
                | Op::UseItem { .. }
                | Op::TransferItem { .. }
                | Op::PlayCreatureAnimation { .. }
                | Op::Patrol { .. }
                | Op::StopPatrol { .. }
                | Op::SetLootable { .. }
                | Op::Kill { .. }
                | Op::TakeGold { .. }
                | Op::SetXp { .. }
                | Op::TakeXp { .. }
                | Op::RemoveLevel { .. }
                | Op::Heal { .. }
                | Op::SetInvulnerable { .. }
                | Op::SetImmortal { .. }
                | Op::SetAi { .. }
                | Op::ForceRest { .. }
                | Op::Limbo { .. }
                | Op::NpcMove { .. }
                | Op::NpcAttack { .. }
                | Op::StopCombat { .. }
                | Op::SetFaction { .. }
                | Op::Freeze { .. }
                | Op::Unfreeze { .. }
                | Op::PcHeal { .. }
                | Op::PcRaise { .. }
                | Op::PcTeleport { .. }
                | Op::PcGiveGold { .. }
                | Op::PcGiveXp { .. }
                | Op::PcGiveItem { .. }
                    if kind != ObjectKind::Creature =>
                {
                    return Err("Target must be a creature".into());
                }
                Op::ViewInventory { .. }
                | Op::TakeItem { .. }
                | Op::NpcSay { .. }
                | Op::SetAppearance { .. }
                    if !matches!(kind, ObjectKind::Creature | ObjectKind::Placeable) =>
                {
                    return Err("Target must be a creature or placeable".into());
                }
                Op::DisableTrap { .. }
                | Op::SpawnTrapOnObject { .. }
                | Op::SetState { .. }
                | Op::SetFacing { .. }
                | Op::SetVisualTransform { .. }
                | Op::SetPosition { .. }
                | Op::PlayAnimation { .. }
                | Op::SetUseable { .. }
                    if !matches!(kind, ObjectKind::Door | ObjectKind::Placeable) =>
                {
                    return Err("Target must be a door or placeable".into());
                }
                _ => {}
            }
            if matches!(
                command.op,
                Op::ReplyToPlayer { .. }
                    | Op::PcMessage { .. }
                    | Op::PcJournal { .. }
                    | Op::PcTakeItem { .. }
                    | Op::PcBoot { .. }
                    | Op::Fade { .. }
                    | Op::PlaySound { .. }
            ) && (!object.pc || object.dm || object.possessed)
            {
                return Err(CommandError::new(
                    ErrorCode::Protected,
                    "Target must be a PC, not an NPC or DM",
                ));
            }
            if matches!(command.op, Op::SkillCheck { .. })
                && (!object.pc || object.dm || object.possessed)
            {
                return Err(CommandError::new(
                    ErrorCode::Protected,
                    "Skill checks target a player, not an NPC or DM",
                ));
            }
            if object.pc && matches!(command.op, Op::NpcAttack { faction: Some(_), .. }) {
                return Err("Fighting faction cannot change a PC".into());
            }
            if matches!(command.op, Op::SetStatic { .. }) && kind != ObjectKind::Placeable {
                return Err("Target must be a placeable".into());
            }
            if matches!(command.op, Op::PcRaise { .. }) && !object.dead {
                return Err("Target is already alive".into());
            }
            if matches!(command.op, Op::SetFacing { .. }) && kind == ObjectKind::Door {
                return Err(CommandError::new(
                    ErrorCode::Disabled,
                    "Native door facing is unsupported; use visual rotation",
                ));
            }
            if let Op::PlayAnimation { name, .. } = command.op {
                if kind == ObjectKind::Door && !matches!(name, Animation::Open | Animation::Close) {
                    return Err("Doors support open/close only".into());
                }
                if kind == ObjectKind::Door && name == Animation::Open && object.lock == Some(true)
                {
                    return Err("Door is locked".into());
                }
            }
            let total = match command.op {
                Op::PcGiveGold { amount, .. } => object.gold.map(|old| (old, amount)),
                Op::PcGiveXp { amount, .. } => object.xp.map(|old| (old, amount)),
                _ => None,
            };
            if total.is_some_and(|(old, n)| old.checked_add(n as i32).is_none()) {
                return Err("Grant would overflow the reported total".into());
            }
            if let Op::NpcAttack { victim, .. } = &command.op {
                let (kind, object) = find_target(&s, area, victim)?;
                if victim == target || kind != ObjectKind::Creature {
                    return Err("Victim must be another creature in this area".into());
                }
                if object.dead {
                    return Err(CommandError::new(ErrorCode::Dead, "Victim is dead"));
                }
            }
        }
        if s.queue.len() >= MAX_QUEUE {
            return Err("Too many commands are waiting for the game".into());
        }
        if matches!(command.op, Op::SkillCheck { .. })
            && s.skill_waiting.len()
                + s.queue.iter().filter(|q| matches!(q.command.op, Op::SkillCheck { .. })).count()
                >= 16
        {
            return Err("At most 16 skill requests may await rolls".into());
        }
        if command.id.is_empty() {
            if s.prefix.is_empty() {
                let mut bytes = [0; 8];
                getrandom::fill(&mut bytes).map_err(|e| format!("Command id randomness: {e}"))?;
                s.prefix = format!("{:016x}", u64::from_ne_bytes(bytes));
            }
            loop {
                s.next += 1;
                command.id = format!("{}-c{}", s.prefix, s.next);
                if !s.accepted.contains_key(&command.id)
                    && !s.command_ids.as_ref().is_some_and(|j| j.contains(&command.id))
                {
                    break;
                }
            }
        }
        // Never silently forget an id and allow a retry to execute twice.
        if s.accepted.len() >= 65_536 {
            return Err("Live session operation limit reached; restart the listener".into());
        }
        let id = command.id.clone();
        let boot = s.boot.clone();
        if let Some(journal) = &mut s.command_ids {
            journal.select_boot(&boot)?;
            journal.reserve(&id)?;
        }
        let audit = match &command.op {
            Op::RunScript { script, .. } => {
                Some(format!("Live: run_script {id} on {}: {script}", command.area))
            }
            Op::RunChunk { code, .. } => Some(format!(
                "Live: run_chunk {id} on {}: {}",
                command.area,
                code.chars().take(120).collect::<String>().escape_debug()
            )),
            _ => None,
        };
        if let Some(message) = audit {
            log::info!("{message}");
            s.logs.push_back(message);
            while s.logs.len() > MAX_QUEUE {
                s.logs.pop_front();
            }
        }
        let drafted = assistant::stage(&mut s, &command, scope, now_ms)?;
        s.accepted.insert(id.clone(), command.clone());
        s.owners.insert(id.clone(), scope.name.clone());
        if drafted {
            return Ok(id);
        }
        s.queue.push_back(Queued { command, sent: false, players, queued_ms: now_ms, ttl_ms });
        if let Some(kick) = &s.kick {
            let _ = kick.send(());
        }
        Ok(id)
    }

    pub fn result(&self, id: &str) -> Option<CommandResult> {
        let s = self.lock();
        if let Some(mirror) = &s.mirror {
            return mirror.result(id);
        }
        s.results
            .iter()
            .find(|(_, r)| r.id == id)
            .map(|(_, r)| r.clone())
            .or_else(|| assistant::pending(&s, id))
            .map(|mut result| {
                result.by = s.owners.get(id).filter(|by| by.as_str() != "local").cloned();
                result
            })
    }

    pub fn area(&self, area: &str) -> Option<AreaLive> {
        self.lock().areas.get(&area.to_ascii_lowercase()).cloned()
    }

    pub fn boot(&self) -> String {
        self.lock().boot.clone()
    }

    /// Cheap report identity for UI cache invalidation. Does not expire commands;
    /// callers that previously relied on `status` for expiry still call `expire`.
    pub fn revision(&self) -> (String, u64) {
        let state = self.lock();
        (state.boot.clone(), state.mirror.as_ref().map_or(state.seq, |m| m.revision))
    }

    /// Latest event revision, including log lines received between game reports.
    pub fn event_cursor(&self) -> u64 {
        self.lock().events.last()
    }

    /// Find an object across areas; a newer report wins during an area transition.
    pub fn locate(&self, id: &str) -> Option<(String, ObjectKind, LiveObject)> {
        let state = self.lock();
        state
            .areas
            .iter()
            .filter_map(|(area, live)| {
                let (kind, object) = live.find(id)?;
                Some((live.received_ms, area, kind, object))
            })
            .max_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)))
            .map(|(_, area, kind, object)| (area.clone(), kind, object.clone()))
    }

    /// Expire commands while an offline game's view is still open.
    pub fn expire(&self, now_ms: u64) {
        expire(&mut self.lock(), now_ms);
    }

    /// Ids of objects that queued commands act on.
    pub fn pending_targets(&self) -> Vec<String> {
        let s = self.lock();
        if let Some(mirror) = &s.mirror {
            return mirror
                .queue
                .iter()
                .filter_map(|q| q.command.target().map(str::to_owned))
                .collect();
        }
        s.queue.iter().filter_map(|q| q.command.target().map(str::to_owned)).collect()
    }

    pub fn status(&self, now_ms: u64) -> Value {
        self.status_scoped(now_ms, &crate::access::Scope::default())
    }

    pub fn status_scoped(&self, now_ms: u64, scope: &crate::access::Scope) -> Value {
        let mut s = self.lock();
        expire(&mut s, now_ms);
        json!({
            "connected": s.mirror.as_ref().map_or(!s.boot.is_empty(), |m| m.game.connected),
            "redis": s.redis.as_ref().map(|state| crate::bridge::status(state)),
            "boot": s.boot,
            "seq": s.seq,
            "delta": s.delta,
            "factions": s.factions,
            "watched_areas": s.watch.reported,
            "watch_error": s.watch.error,
            "resyncPending": s.resync.is_some(),
            "resyncs": s.resyncs,
            "ageMs": s.mirror.as_ref().map(|m| m.game.age_ms).or_else(|| (!s.boot.is_empty()).then(|| now_ms.saturating_sub(s.received_ms))),
            "areas": s.areas.iter().map(|(name, a)| json!({
                "area": name,
                "ageMs": now_ms.saturating_sub(a.received_ms),
                "creatures": a.creatures.len(),
                "placeables": a.placeables.len(),
                "doors": a.doors.len(),
                "items": a.items.len(),
                "waypoints": a.waypoints.len(),
                "sounds": a.sounds.len(),
                "encounters": a.encounters.len(),
                "stores": a.stores.len(),
                "triggers": a.triggers.len(),

            })).collect::<Vec<_>>(),
            "queued": s.queue.iter().filter(|q| scope.unrestricted() || s.owners.get(&q.command.id) == Some(&scope.name)).map(|q| attributed_command(&s,&q.command)).collect::<Vec<_>>(),
            "results": s.results.iter().rev().filter(|(c, _)| scope.unrestricted() || s.owners.get(&c.id) == Some(&scope.name)).map(|(c, r)| json!({"command": attributed_command(&s,c), "result": r})).collect::<Vec<_>>(),
        })
    }

    /// An area's report for a client, with its age.
    pub fn area_json(&self, area: &str, now_ms: u64) -> Option<Value> {
        let s = self.lock();
        let a = s.areas.get(&area.to_ascii_lowercase())?;
        Some(json!({
            "area": area,
            "ageMs": now_ms.saturating_sub(a.received_ms),
            "factions": s.factions,
            "fullAgeMs": (a.has_full || a.full_ms != 0).then(|| now_ms.saturating_sub(a.full_ms)),
            "creatures": a.creatures,
            "placeables": a.placeables,
            "size": a.size,
            "doors": a.doors,
            "items": a.items,
            "waypoints": a.waypoints,
            "sounds": a.sounds,
            "encounters": a.encounters,
            "stores": a.stores,
            "triggers": a.triggers,

        }))
    }
}

fn find_target<'a>(
    s: &'a State,
    area: &'a AreaLive,
    id: &str,
) -> Result<(ObjectKind, &'a LiveObject), CommandError> {
    area.find(id).ok_or_else(|| {
        if s.areas.values().any(|a| a.find(id).is_some()) {
            CommandError::new(ErrorCode::OtherArea, "Target is in another area")
        } else {
            CommandError::new(ErrorCode::NotFound, "Target is not in the latest reports")
        }
    })
}

fn policy_error(policy: Policy, op: &Op) -> Option<&'static str> {
    match op {
        op if op.is_player() && !policy.players => {
            Some("Player operations are off in Options > Live")
        }
        Op::RunScript { .. } | Op::SpawnJson { .. } if !policy.scripts => {
            Some("Module scripts are off in Options › Live")
        }
        Op::RunChunk { .. } if !policy.chunks => Some("NWScript chunks are off in Options › Live"),
        _ => None,
    }
}

fn receive_results(
    s: &mut State,
    results: Vec<CommandResult>,
    ack: Vec<String>,
    seq: u64,
    now_ms: u64,
) {
    for r in results {
        let command = if let Some(i) = s.queue.iter().position(|q| q.command.id == r.id) {
            Some(s.queue.remove(i).expect("position is in range").command)
        } else if let Some((command, _)) = s.skill_waiting.get(&r.id).cloned() {
            if seq < s.seq || s.results.iter().any(|(_, old)| old == &r) {
                continue;
            }
            Some(command)
        } else {
            // A delayed result may resolve an ACK-only unknown outcome. Never
            // overwrite a known final result with a duplicate or older frame.
            s.results
                .iter()
                .find(|(_, old)| old.id == r.id && old.code == Some(ErrorCode::Unknown))
                .map(|(command, _)| command.clone())
        };
        if let Some(command) = command {
            if r.ok && r.pending {
                if let Op::SkillCheck { timeout, .. } = command.op {
                    s.skill_waiting.entry(r.id.clone()).or_insert_with(|| {
                        (command.clone(), now_ms + u64::from(timeout) * 1000 + 5000)
                    });
                }
            } else {
                s.skill_waiting.remove(&r.id);
            }
            push_result(s, command, r);
        }
    }
    for id in ack {
        if let Some(i) = s.queue.iter().position(|q| q.command.id == id) {
            let q = s.queue.remove(i).expect("in range");
            push_result(
                s,
                q.command,
                failure(&id, ErrorCode::Unknown, "unknown: acknowledged; result not received"),
            );
        }
    }
}

fn expire(s: &mut State, now_ms: u64) {
    if let Some(mirror) = &mut s.mirror {
        mirror.expire(now_ms);
        return;
    }
    let timed_out: Vec<_> = s
        .skill_waiting
        .iter()
        .filter(|(_, (_, until))| now_ms >= *until)
        .map(|(id, _)| id.clone())
        .collect();
    for id in timed_out {
        if let Some((command, _)) = s.skill_waiting.remove(&id) {
            push_result(
                s,
                command,
                failure(&id, ErrorCode::Unknown, "unknown: no final skill result received"),
            );
        }
    }
    let mut i = 0;
    while i < s.queue.len() {
        if now_ms.saturating_sub(s.queue[i].queued_ms) >= s.queue[i].ttl_ms {
            let q = s.queue.remove(i).expect("in range");
            let result = undelivered(&q.command.id, q.sent, "not acknowledged in time");
            push_result(s, q.command, result);
        } else {
            i += 1;
        }
    }
}

fn undelivered(id: &str, sent: bool, reason: &str) -> CommandResult {
    if sent {
        failure(id, ErrorCode::Unknown, &format!("unknown: {reason}; command may have executed"))
    } else {
        failure(id, ErrorCode::Expired, &format!("expired: {reason}; command was never sent"))
    }
}

fn failure(id: &str, code: ErrorCode, err: &str) -> CommandResult {
    CommandResult {
        id: id.into(),
        ok: false,
        err: err.into(),
        obj: String::new(),
        code: Some(code),
        faction: None,
        ..CommandResult::default()
    }
}

fn attributed_command(s: &State, command: &Command) -> Value {
    let mut value = serde_json::to_value(command).expect("validated command serializes");
    if let Some(by) = s.owners.get(&command.id).filter(|by| by.as_str() != "local") {
        value["by"] = json!(by);
    }
    value
}

fn game_commands(s: &mut State) -> Value {
    let mut commands: Vec<Value> = s
        .queue
        .iter()
        .take(MAX_QUEUE - usize::from(s.resync.is_some()) - s.watch.pending.len())
        .scan((0usize, 0usize), |budget, q| {
            let mut value = attributed_command(s, &q.command);
            // Consent and identity come from the service, never caller JSON.
            if s.policy.players && q.players {
                value["players"] = json!(true);
            }
            let bytes = crate::bridge::game_json(&value).len();
            let capture = usize::from(matches!(q.command.op, Op::CaptureObject { .. }));
            if budget.0 + bytes > 1024 * 1024 || budget.1 + capture > 4 {
                return None;
            }
            budget.0 += bytes;
            budget.1 += capture;
            Some(value)
        })
        .collect();
    // Only commands actually included in this bounded hand-off may have run.
    for q in s.queue.iter_mut().take(commands.len()) {
        q.sent = true;
    }
    if let Some(id) = &s.resync {
        commands.insert(0, json!({"id":id,"op":"resync"}));
    }
    commands.extend(s.watch.pending.iter().cloned());
    let boot_prefix = format!("{}:", s.boot);
    let received: Vec<_> = s
        .results
        .iter()
        .filter_map(|(_, result)| result.receipt.as_ref())
        .filter(|receipt| receipt.starts_with(&boot_prefix))
        .collect();
    json!({"v":1,"boot":s.boot,"prefix":s.server.as_ref().map(|c| c.prefix.as_str()).unwrap_or_default(),"commands":commands,"received":received})
}

fn push_result(s: &mut State, command: Command, mut result: CommandResult) {
    result.by = s.owners.get(&command.id).filter(|by| by.as_str() != "local").cloned();
    let draft_answer = assistant::completed(s, &command, &result);
    if let Some(audit) = &s.audit {
        let cmd = serde_json::to_value(&command).unwrap_or_default();
        let code = serde_json::to_value(result.code).unwrap_or_default();
        audit.record(
            s.owners.get(&command.id).map(String::as_str).unwrap_or("local"),
            cmd["op"].as_str().unwrap_or("unknown"),
            command.target(),
            if result.ok { "ok" } else { code.as_str().unwrap_or("failed") },
            if result.pending { "progress" } else { "completion" },
        );
    }
    gateway::record_result(&mut s.gateway, &result);
    let answer = serde_json::to_value(&result).unwrap_or_default();
    s.events.command(&command.area, &answer, crate::now_ms());
    s.results.retain(|(_, old)| old.id != result.id);
    s.results.push_back((command, result));
    while s.results.len() > MAX_RESULTS {
        s.results.pop_front();
    }
    if let Some((command, result)) = draft_answer {
        push_result(s, command, result);
    }
}

/// A GIT instance as reconciliation sees it.
#[derive(Debug, Clone, Copy)]
pub struct GitObject<'a> {
    pub kind: ObjectKind,
    pub template: &'a str,
    pub position: Vec3,
    pub is_static: bool,
}

/// How the game's report lines up with the GIT.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reconciled {
    /// Per GIT object (same order): the reported object it is, if any.
    pub matched: Vec<Option<(ObjectKind, usize)>>,
    /// GIT objects to hide: creatures (the game reports where they are now)
    /// and placeables, doors and items the game no longer has.
    pub hidden: Vec<bool>,
    /// Reported objects with no GIT instance: every creature and whatever
    /// was made while the game ran. (kind, index in its reported list)
    pub extra: Vec<(ObjectKind, usize)>,
}

fn reported(live: &AreaLive, kind: ObjectKind) -> &[LiveObject] {
    match kind {
        ObjectKind::Creature => &live.creatures,
        ObjectKind::Placeable => &live.placeables,
        ObjectKind::Door => &live.doors,
        ObjectKind::Item => &live.items,
        ObjectKind::Waypoint => &live.waypoints,
        ObjectKind::Sound => &live.sounds,
        ObjectKind::Encounter => &live.encounters,
        ObjectKind::Store => &live.stores,
        ObjectKind::Trigger => &live.triggers,
    }
}

/// Pairs placeables, doors and items by blueprint and position (they do not
/// move); a non-static one the game no longer reports was destroyed. Static
/// placeables are scenery: never taken for destroyed.
pub fn reconcile(git: &[GitObject], live: &AreaLive) -> Reconciled {
    let mut used: HashMap<ObjectKind, Vec<bool>> = HashMap::new();
    let mut out = Reconciled::default();
    for g in git {
        let (matched, hidden) = match g.kind {
            ObjectKind::Creature => (None, true),
            ObjectKind::Placeable | ObjectKind::Door | ObjectKind::Item => {
                let list = reported(live, g.kind);
                let used = used.entry(g.kind).or_insert_with(|| vec![false; list.len()]);
                let best = list
                    .iter()
                    .enumerate()
                    .filter(|(i, o)| !used[*i] && o.resref.eq_ignore_ascii_case(g.template))
                    .map(|(i, o)| (i, o.position().truncate().distance(g.position.truncate())))
                    .filter(|(_, d)| *d <= MATCH_DISTANCE)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match best {
                    Some((i, _)) => {
                        used[i] = true;
                        (Some((g.kind, i)), false)
                    }
                    None => (None, (live.has_full || live.full_ms != 0) && !g.is_static),
                }
            }
            _ => (None, false),
        };
        out.matched.push(matched);
        out.hidden.push(hidden);
    }
    out.extra.extend((0..live.creatures.len()).map(|i| (ObjectKind::Creature, i)));
    for kind in [ObjectKind::Placeable, ObjectKind::Door, ObjectKind::Item] {
        let n = reported(live, kind).len();
        let used = used.get(&kind);
        out.extra.extend((0..n).filter(|i| used.is_none_or(|u| !u[*i])).map(|i| (kind, i)));
    }
    out
}

#[cfg(test)]
#[path = "store_ops_tests.rs"]
mod ops_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn post(store: &LiveStore, body: Value, now: u64) -> Value {
        store.sync(body.to_string().as_bytes(), now).unwrap()
    }

    fn report(boot: &str, seq: u64, full: bool) -> Value {
        json!({"v":1,"boot":boot,"seq":seq,"areas":[{"area":"market","full":full,
            "cre":[{"id":"7f01","r":"guard","name":"Guard","x":3.0,"y":4.0,"hp":[5,9]}],
            "plc":[{"id":"7f02","r":"barrel","x":10.0,"y":10.05,"open":false}]}]})
    }

    fn spawn() -> Command {
        Command {
            id: String::new(),
            area: "market".into(),
            op: Op::Spawn {
                kind: SpawnKind::Creature,
                r: "guard".into(),
                x: 1.,
                y: 2.,
                z: 0.,
                f: 90.,
                count: 1,
                faction: None,
            },
        }
    }

    #[test]
    fn direct_enqueue_enforces_all_operation_groups_even_with_global_policy() {
        use crate::access::{Access, ClientGroup, ClientToken};
        let access = Access::new(
            "d".repeat(32),
            &[ClientToken {
                name: "reader".into(),
                token: "r".repeat(32),
                groups: vec![ClientGroup::Read],
                ..ClientToken::default()
            }],
        )
        .unwrap();
        let store = LiveStore::default();
        post(&store, report("scope", 1, true), 10);
        store.set_policy(Policy { scripts: true, chunks: true, players: true });
        for op in [
            json!({"op":"destroy","target":"7f02"}),
            json!({"op":"run_script","script":"mglive_probe"}),
            json!({"op":"run_chunk","code":"int x = 1;"}),
            json!({"op":"npc_move","target":"7f01","x":1,"y":2,"z":0}),
            json!({"op":"npc_say","target":"7f01","text":"hello"}),
        ] {
            let mut value = op;
            value["area"] = json!("market");
            let command = serde_json::from_value(value).unwrap();
            assert_eq!(
                store.enqueue_scoped(command, 11, access.scope(1)).unwrap_err().code,
                ErrorCode::Forbidden
            );
        }
        assert!(store.commands(11)["commands"].as_array().unwrap().is_empty());
    }

    #[test]
    fn server_factions_are_validated_preserved_on_delta_and_reset_on_boot() {
        let store = LiveStore::default();
        let mut frame = report("server-factions", 1, true);
        frame["factions"] = json!([{"id":0,"name":"PC"},{"id":6,"name":"Session 1"}]);
        post(&store, frame.clone(), 10);
        assert_eq!(store.status(10)["factions"], frame["factions"]);
        assert_eq!(store.area_json("market", 10).unwrap()["factions"], frame["factions"]);
        assert_eq!(store.factions().unwrap()[1].id, 6);
        post(&store, report("server-factions", 2, false), 20);
        assert_eq!(store.factions().unwrap().len(), 2);
        for bad in
            [json!([{"id":65536,"name":"x"}]), json!([{"id":0,"name":"x"},{"id":0,"name":"y"}])]
        {
            frame["factions"] = bad;
            assert!(store.sync(frame.to_string().as_bytes(), 30).is_err());
        }
        post(&store, report("old-server", 1, true), 40);
        assert_eq!(store.factions(), None);
    }

    #[test]
    fn factions_and_relationships_are_optional_validated_report_fields() {
        let store = LiveStore::default();
        let mut full = report("factions", 1, true);
        let o = &mut full["areas"][0]["cre"][0];
        o["faction"] = json!(7);
        o["friendly"] = json!(["abc"]);
        post(&store, full.clone(), 10);
        let before = store.area("market").unwrap();
        assert_eq!(before.creatures[0].faction, Some(7));
        assert_eq!(before.creatures[0].friendly, Some(vec!["abc".into()]));
        full["seq"] = json!(2);
        full["areas"][0]["cre"][0]["friendly"] = json!(["invalid id"]);
        assert!(store.sync(full.to_string().as_bytes(), 20).is_err());
        full["areas"][0]["cre"][0]["friendly"] = json!(vec!["abc"; MAX_OBJECTS + 1]);
        assert!(store.sync(full.to_string().as_bytes(), 20).is_err());
        assert_eq!(store.area("market").unwrap(), before);
        post(&store, report("factions", 2, false), 20);
        let old = store.area("market").unwrap();
        assert_eq!(old.creatures[0].faction, None);
        assert_eq!(old.creatures[0].friendly, None);
    }

    #[test]
    fn npc_equipment_look_replaces_cached_equipment_in_full_and_delta_reports() {
        for delta in [false, true] {
            let store = LiveStore::default();
            let mut first = report("npc-look", 1, true);
            first["areas"][0]["cre"][0]["look"] =
                json!({"Equip_ItemList":{"type":"list","value":[{"__struct_id":2}]}});
            first["areas"][0]["cre"][0]["lookHash"] = json!(11);
            post(&store, first.clone(), 10);
            let mut next = first.clone();
            next["seq"] = json!(2);
            next["areas"][0]["full"] = json!(!delta);
            if delta {
                next["base_seq"] = json!(1);
            }
            next["areas"][0]["cre"][0]["look"]["Equip_ItemList"]["value"] = json!([]);
            next["areas"][0]["cre"][0]["lookHash"] = json!(12);
            post(&store, next, 20);
            let changed = store.area("market").unwrap().creatures.remove(0);
            assert!(!changed.pc);
            assert_eq!(changed.look_hash, Some(12));
            assert_eq!(changed.look.unwrap()["Equip_ItemList"]["value"], json!([]));
            let mut quick = report("npc-look", 3, false);
            if delta {
                quick["base_seq"] = json!(2);
                // Delta objects are complete snapshots; omit the unchanged object.
                quick["areas"][0]["cre"] = json!([]);
            }
            post(&store, quick, 30);
            let preserved = store.area("market").unwrap().creatures.remove(0);
            assert_eq!(preserved.look_hash, Some(12));
            assert_eq!(preserved.look.unwrap()["Equip_ItemList"]["value"], json!([]));
        }
    }

    #[test]
    fn player_look_quick_reports_limits_and_truncation() {
        let store = LiveStore::default();
        let mut full = report("look", 1, true);
        let o = &mut full["areas"][0]["cre"][0];
        o["pc"] = json!(true);
        o["look"] = json!({"__data_type":"UTC ","ArmorPart_RFoot":{"type":"byte","value":1},"xBodyPart_LFoot":{"type":"word","value":300}});
        o["lookHash"] = json!(-7);
        o["combat"] = json!(true);
        o["hostile"] = json!(["abc"]);
        o["target"] = json!("abc");
        post(&store, full.clone(), 10);
        let original = store.area("market").unwrap().creatures[0].clone();
        let mut quick = report("look", 2, false);
        quick["areas"][0]["cre"][0]["x"] = json!(9);
        post(&store, quick, 20);
        let o = store.area("market").unwrap().creatures.remove(0);
        assert_eq!(o.look, original.look);
        assert_eq!(o.look_hash, Some(-7));
        assert_eq!(o.x, 9.);
        assert!(!o.combat);
        full["seq"] = json!(3);
        full["areas"][0]["cre"][0]["lookHash"] = json!(8);
        full["areas"][0]["cre"][0]["look"]["ArmorPart_RFoot"]["value"] = json!(2);
        post(&store, full.clone(), 30);
        let before = store.area("market").unwrap();
        assert_eq!(before.creatures[0].look_hash, Some(8));
        let bytes = full.to_string();
        for cut in 0..bytes.len() {
            assert!(store.sync(&bytes.as_bytes()[..cut], 40).is_err(), "cut {cut}");
        }
        full["areas"][0]["cre"][0]["look"] = json!({"large":"x".repeat(64 * 1024)});
        assert!(store.sync(full.to_string().as_bytes(), 50).unwrap_err().contains("64 KiB"));
        assert_eq!(store.area("market").unwrap(), before, "refusal is atomic");
        post(&store, report("reboot", 1, false), 60);
        assert!(store.area("market").unwrap().creatures[0].look.is_none());
    }

    #[test]
    fn move_and_attack_hold_have_legacy_defaults_and_bounded_timeouts() {
        for old in [
            json!({"area":"market","op":"npc_attack","target":"a","victim":"b"}),
            json!({"area":"market","op":"npc_move","target":"a","x":1,"y":2,"z":0}),
        ] {
            let command: Command = serde_json::from_value(old.clone()).unwrap();
            assert!(matches!(
                command.op,
                Op::NpcAttack { hold: false, timeout: 30, .. }
                    | Op::NpcMove { hold: false, timeout: 30, .. }
            ));
            command.validate().unwrap();
            for timeout in [0, 1, 120, 121] {
                let mut value = old.clone();
                value["hold"] = json!(true);
                value["timeout"] = json!(timeout);
                let command: Command = serde_json::from_value(value).unwrap();
                assert_eq!(command.validate().is_ok(), (1..=120).contains(&timeout));
            }
            for (key, value) in [("hold", json!("yes")), ("timeout", json!(1.5))] {
                let mut bad = old.clone();
                bad[key] = value;
                assert!(serde_json::from_value::<Command>(bad).is_err());
            }
        }
    }

    #[test]
    fn structured_refusals_and_results() {
        let store = LiveStore::default();
        let mut state = report("b", 1, true);
        state["areas"][0]["cre"][0]["dead"] = json!(true);
        state["areas"][0]["cre"][0]["pc"] = json!(true);
        state["areas"].as_array_mut().unwrap().push(json!({"area":"other","cre":[{"id":"ab"}]}));
        post(&store, state, 0);
        let command = |target: &str| Command {
            id: String::new(),
            area: "market".into(),
            op: Op::NpcMove {
                target: target.into(),
                x: 0.,
                y: 0.,
                z: 0.,
                run: false,
                hold: false,
                timeout: 30,
            },
        };
        for (target, code) in [
            ("bad-id", ErrorCode::Invalid),
            ("ff", ErrorCode::NotFound),
            ("ab", ErrorCode::OtherArea),
            ("7f01", ErrorCode::Dead),
        ] {
            assert_eq!(store.enqueue(command(target), 0).unwrap_err().code, code);
        }
        let mut c = command("7f01");
        c.op = Op::Destroy { target: "7f01".into() };
        assert_eq!(store.enqueue(c.clone(), 0).unwrap_err().code, ErrorCode::Protected);
        let mut state = report("b", 2, true);
        state["areas"][0]["cre"][0]["dead"] = json!(true);
        post(&store, state, 0);
        let id = store.enqueue(c, 0).unwrap();
        store.expire(COMMAND_TTL_MS);
        assert_eq!(store.result(&id).unwrap().code, Some(ErrorCode::Expired));
        let id = store.enqueue(spawn(), COMMAND_TTL_MS).unwrap();
        post(
            &store,
            json!({"v":1,"boot":"b","seq":3,"results":[{"id":id,"ok":false,"code":"compile_error","err":"bad script"}]}),
            COMMAND_TTL_MS + 1,
        );
        assert_eq!(store.result(&id).unwrap().code, Some(ErrorCode::CompileError));
        for code in [
            "not_found",
            "other_area",
            "dead",
            "protected",
            "invalid",
            "disabled",
            "unknown",
            "ambiguous",
            "unknown_script",
            "compile_error",
            "expired",
        ] {
            let result: CommandResult =
                serde_json::from_value(json!({"id":"a","code":code})).unwrap();
            assert_eq!(serde_json::to_value(result).unwrap()["code"], code);
        }
    }

    #[test]
    fn lost_failure_is_unknown_until_result_and_late_ack_never_claims_success() {
        let store = LiveStore::default();
        post(&store, report("b", 1, true), 0);
        let id = store.enqueue(spawn(), 1).unwrap();
        post(&store, json!({"v":1,"boot":"b","seq":2,"ack":[id]}), 2);
        let missing = store.result(&id).unwrap();
        assert!(!missing.ok);
        assert_eq!(missing.code, Some(ErrorCode::Unknown));
        assert!(missing.err.starts_with("unknown:"));
        let answer =
            json!({"id":id,"ok":false,"code":"invalid","err":"spawn failed","receipt":"b:1"});
        let reply = post(&store, json!({"v":1,"boot":"b","seq":3,"results":[answer]}), 3);
        assert_eq!(store.result(&id).unwrap().err, "spawn failed");
        assert_eq!(reply["received"], json!(["b:1"]));
        post(&store, json!({"v":1,"boot":"b","seq":4,"ack":[id]}), 4);
        assert_eq!(store.result(&id).unwrap().code, Some(ErrorCode::Invalid));
        let expired = store.enqueue(spawn(), 5).unwrap();
        store.expire(COMMAND_TTL_MS + 5);
        post(&store, json!({"v":1,"boot":"b","seq":5,"ack":[expired]}), COMMAND_TTL_MS + 6);
        assert_eq!(store.result(&expired).unwrap().code, Some(ErrorCode::Expired));
        assert!(!store.result(&expired).unwrap().ok);
        let reboot = post(&store, report("new", 1, true), COMMAND_TTL_MS + 7);
        assert_eq!(reboot["received"], json!([]));
    }

    #[test]
    fn script_policy_targets_kick_and_ack_without_result() {
        let store = LiveStore::default();
        let mut state = report("b1", 1, true);
        state["areas"][0]["cre"][0]["pc"] = json!(true);
        post(&store, state, 0);
        let script = Command {
            id: "script".into(),
            area: "market".into(),
            op: Op::RunScript { script: "mglive_probe".into(), target: Some("7f01".into()) },
        };
        let (kick, receive) = std::sync::mpsc::channel();
        store.set_kick(kick);
        assert_eq!(store.enqueue(script.clone(), 0).unwrap(), "script");
        receive.recv_timeout(std::time::Duration::from_millis(100)).unwrap();
        post(&store, json!({"v":1,"boot":"b1","seq":2,"ack":["script"]}), 1);
        assert_eq!(
            store.status(1)["results"][0]["result"],
            json!({"id":"script","ok":false,"code":"unknown","err":"unknown: acknowledged; result not received","obj":""})
        );
        assert!(store.commands(1)["commands"].as_array().unwrap().is_empty());
        store.enqueue(script, 1).unwrap();
        assert!(receive.try_recv().is_err(), "acknowledged retry must not queue again");
        let mut chunk: Command = serde_json::from_value(
            json!({"area":"market","op":"run_chunk","code":"SetName(OBJECT_SELF, \"test\");"}),
        )
        .unwrap();
        assert!(matches!(chunk.op, Op::RunChunk { wrap: true, target: None, .. }));
        assert_eq!(
            store.enqueue(chunk.clone(), 1).unwrap_err().error,
            "NWScript chunks are off in Options › Live"
        );
        store.set_policy(Policy { scripts: true, chunks: true, players: false });
        chunk.id = "chunk".into();
        store.enqueue(chunk, 1).unwrap();
        store.set_policy(Policy { scripts: false, chunks: false, players: false });
        assert!(store.commands(1)["commands"].as_array().unwrap().is_empty());
        assert_eq!(store.status(1)["results"][0]["result"]["ok"], false);
        let bad_target = Command {
            id: String::new(),
            area: "market".into(),
            op: Op::RunScript { script: "mglive_probe".into(), target: Some("abcd".into()) },
        };
        assert_eq!(
            store.enqueue(bad_target.clone(), 1).unwrap_err().error,
            "Module scripts are off in Options › Live"
        );
        store.set_policy(Policy::default());
        assert!(store.enqueue(bad_target, 1).unwrap_err().contains("not in the latest reports"));
    }

    #[test]
    fn script_fields_and_compiler_errors_are_bounded() {
        for op in [
            Op::RunScript { script: "not-script".into(), target: None },
            Op::RunScript { script: "x".repeat(17), target: None },
            Op::RunChunk { code: " ".into(), target: None, wrap: true, return_value: false },
            Op::RunChunk { code: "x".repeat(8001), target: None, wrap: true, return_value: false },
            Op::RunChunk {
                code: "x".into(),
                target: Some("invalid".into()),
                wrap: true,
                return_value: false,
            },
        ] {
            assert!(Command { id: String::new(), area: "market".into(), op }.validate().is_err());
        }
        let store = LiveStore::default();
        post(&store, report("b", 1, true), 0);
        store.set_policy(Policy { scripts: true, chunks: true, players: false });
        let id = store
            .enqueue(
                Command {
                    id: String::new(),
                    area: "market".into(),
                    op: Op::RunChunk {
                        code: "bad".into(),
                        target: None,
                        wrap: true,
                        return_value: false,
                    },
                },
                0,
            )
            .unwrap();
        post(
            &store,
            json!({"v":1,"boot":"b","seq":2,"results":[{"id":id,"ok":false,"err":"error ".repeat(100)}]}),
            1,
        );
        assert_eq!(store.status(1)["results"][0]["result"]["err"].as_str().unwrap().len(), 600);
    }

    #[test]
    fn reports_replace_creatures_every_post_and_the_rest_only_when_full() {
        let store = LiveStore::default();
        post(&store, report("b1", 1, true), 1000);
        let mut quick = report("b1", 2, false);
        quick["areas"][0]["cre"] = json!([]);
        quick["areas"][0]["plc"] = json!([]);
        post(&store, quick, 2000);
        let a = store.area("market").unwrap();
        assert!(a.creatures.is_empty());
        assert_eq!(a.placeables.len(), 1, "a quick report keeps the last full one");
        assert_eq!((a.received_ms, a.full_ms), (2000, 1000));
    }

    #[test]
    fn commands_ride_replies_until_acknowledged() {
        let store = LiveStore::default();
        assert!(store.enqueue(spawn(), 0).is_err(), "no game yet");
        post(&store, report("b1", 1, true), 0);
        let id = store.enqueue(spawn(), 10).unwrap();
        let reply = post(&store, report("b1", 2, false), 20);
        assert_eq!(reply["commands"][0]["id"], id.as_str());
        assert_eq!(reply["commands"][0]["op"], "spawn");
        assert_eq!(reply["commands"][0]["kind"], "creature");
        let again = post(&store, report("b1", 3, false), 30);
        assert_eq!(again["commands"].as_array().unwrap().len(), 1, "resent until acknowledged");
        let mut ack = report("b1", 4, false);
        ack["ack"] = json!([id]);
        ack["results"] = json!([{"id": id, "ok": true, "obj": "7f09"}]);
        let done = post(&store, ack, 40);
        assert!(done["commands"].as_array().unwrap().is_empty());
        assert_eq!(store.status(40)["results"][0]["result"]["obj"], "7f09");
    }

    #[test]
    fn a_retried_operation_is_queued_once() {
        let store = LiveStore::default();
        post(&store, report("b1", 1, true), 0);
        let mut c = spawn();
        c.id = "op-1".into();
        assert_eq!(store.enqueue(c.clone(), 0).unwrap(), "op-1");
        assert_eq!(store.enqueue(c, 0).unwrap(), "op-1");
        assert_eq!(store.status(0)["queued"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unacknowledged_commands_expire_and_a_restart_drops_everything() {
        let store = LiveStore::default();
        post(&store, report("b1", 1, true), 0);
        store.enqueue(spawn(), 0).unwrap();
        let late = post(&store, report("b1", 2, false), COMMAND_TTL_MS);
        assert!(late["commands"].as_array().unwrap().is_empty());
        assert!(
            store.status(0)["results"][0]["result"]["err"].as_str().unwrap().contains("expired")
        );

        store.enqueue(spawn(), COMMAND_TTL_MS).unwrap();
        let restarted = post(&store, json!({"v":1,"boot":"b2","seq":1}), COMMAND_TTL_MS + 1);
        assert!(restarted["commands"].as_array().unwrap().is_empty());
        assert!(store.area("market").is_none(), "a restarted server's areas are forgotten");
    }

    #[test]
    fn an_older_report_does_not_overwrite_a_newer_one() {
        let store = LiveStore::default();
        post(&store, report("b1", 5, true), 0);
        let mut old = report("b1", 4, true);
        old["areas"][0]["cre"] = json!([]);
        post(&store, old, 10);
        assert_eq!(store.area("market").unwrap().creatures.len(), 1);
    }

    #[test]
    fn bad_posts_are_refused_without_panicking() {
        let store = LiveStore::default();
        for body in [&b""[..], b"{", b"[]", b"{\"v\":2,\"boot\":\"x\"}", b"{\"v\":1}", b"\xff\xfe"]
        {
            assert!(store.sync(body, 0).is_err(), "{:?}", String::from_utf8_lossy(body));
        }
        let long = "x".repeat(MAX_TEXT + 1);
        let mut r = report("b1", 1, true);
        r["areas"][0]["cre"][0]["name"] = json!(long);
        assert!(store.sync(r.to_string().as_bytes(), 0).is_err());
        let full = report("b1", 1, true).to_string();
        for cut in 0..full.len() {
            assert!(store.sync(&full.as_bytes()[..cut], 0).is_err());
        }
    }

    #[test]
    fn commands_are_validated() {
        let bad = [
            Op::Spawn {
                kind: SpawnKind::Item,
                r: "Not A Resref".into(),
                x: 0.,
                y: 0.,
                z: 0.,
                f: 0.,
                count: 1,
                faction: None,
            },
            Op::Spawn {
                kind: SpawnKind::Item,
                r: "x".into(),
                x: f32::NAN,
                y: 0.,
                z: 0.,
                f: 0.,
                count: 1,
                faction: None,
            },
            Op::Destroy { target: "OBJECT_SELF".into() },
            Op::NpcSay { target: "7f01".into(), text: " ".into(), volume: SpeechVolume::Talk },
            Op::NpcSay { target: "7f01".into(), text: "a".repeat(201), volume: SpeechVolume::Talk },
            Op::SetState { target: "7f01".into(), open: None, lock: None },
        ];
        for op in bad {
            let c = Command { id: String::new(), area: "a".into(), op };
            assert!(c.validate().is_err(), "{c:?}");
        }
        let ok = Command {
            id: String::new(),
            area: "a".into(),
            op: Op::SetState { target: "7f01".into(), open: Some(true), lock: None },
        };
        assert_eq!(
            serde_json::to_value(&ok).unwrap(),
            json!({"id":"","area":"a","op":"set_state","target":"7f01","open":true})
        );
        let back: Command = serde_json::from_value(
            json!({"area":"a","op":"npc_move","target":"7f01","x":1,"y":2,"z":0}),
        )
        .unwrap();
        assert_eq!(
            back.op,
            Op::NpcMove {
                target: "7f01".into(),
                x: 1.,
                y: 2.,
                z: 0.,
                run: false,
                hold: false,
                timeout: 30
            }
        );
    }

    #[test]
    fn reconciliation_pairs_by_blueprint_and_position() {
        let live = AreaLive {
            full_ms: 1,
            creatures: vec![LiveObject {
                id: "1".into(),
                resref: "guard".into(),
                ..Default::default()
            }],
            placeables: vec![
                LiveObject {
                    id: "2".into(),
                    resref: "barrel".into(),
                    x: 10.0,
                    y: 10.05,
                    ..Default::default()
                },
                LiveObject {
                    id: "3".into(),
                    resref: "chest".into(),
                    x: 5.0,
                    y: 5.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let g = |kind, template, x, y, is_static| GitObject {
            kind,
            template,
            position: Vec3::new(x, y, 0.0),
            is_static,
        };
        let git = [
            g(ObjectKind::Creature, "guard", 3.0, 4.0, false),
            g(ObjectKind::Placeable, "BARREL", 10.0, 10.0, false),
            g(ObjectKind::Placeable, "barrel", 20.0, 20.0, false),
            g(ObjectKind::Placeable, "statue", 1.0, 1.0, true),
            g(ObjectKind::Waypoint, "wp", 0.0, 0.0, false),
        ];
        let r = reconcile(&git, &live);
        assert_eq!(r.matched, vec![None, Some((ObjectKind::Placeable, 0)), None, None, None]);
        assert_eq!(
            r.hidden,
            vec![true, false, true, false, false],
            "creatures and the destroyed barrel"
        );
        assert_eq!(r.extra, vec![(ObjectKind::Creature, 0), (ObjectKind::Placeable, 1)]);

        let before_full = AreaLive { full_ms: 0, ..live };
        assert!(
            !reconcile(&git, &before_full).hidden[2],
            "nothing is destroyed before a full report"
        );
    }

    #[test]
    fn protects_players_possession_area_and_object_types() {
        let store = LiveStore::default();
        let mut r = report("b", 1, true);
        r["areas"][0]["cre"][0]["possessed"] = json!(true);
        post(&store, r, 0);
        let cmd = |op| Command { id: String::new(), area: "market".into(), op };
        assert!(store.enqueue(cmd(Op::Destroy { target: "7f01".into() }), 0).is_err());
        assert!(store.enqueue(cmd(Op::Destroy { target: "abc".into() }), 0).is_err());
        assert!(
            store
                .enqueue(
                    cmd(Op::NpcMove {
                        target: "7f02".into(),
                        x: 0.,
                        y: 0.,
                        z: 0.,
                        run: false,
                        hold: false,
                        timeout: 30
                    }),
                    0
                )
                .is_err()
        );
        assert!(
            store
                .enqueue(
                    cmd(Op::NpcSay {
                        target: "7f02".into(),
                        text: "Hello".into(),
                        volume: SpeechVolume::Talk
                    }),
                    0
                )
                .is_ok()
        );
        let mut c = spawn();
        c.area = "elsewhere".into();
        assert!(store.enqueue(c, 0).is_err());
        for field in ["pc", "dm"] {
            let mut r = report("b", 2, true);
            r["areas"][0]["cre"][0][field] = json!(true);
            post(&store, r, 1);
            assert!(store.enqueue(cmd(Op::Destroy { target: "7f01".into() }), 1).is_err());
        }
    }

    #[test]
    fn retries_survive_ack_only_and_recent_result_eviction() {
        let store = LiveStore::default();
        post(&store, report("b", 1, true), 0);
        let mut first = spawn();
        first.id = "first".into();
        store.enqueue(first.clone(), 0).unwrap();
        post(&store, json!({"v":1,"boot":"b","ack":["first"]}), 0);
        for i in 0..70 {
            let id = store.enqueue(spawn(), i).unwrap();
            post(&store, json!({"v":1,"boot":"b","results":[{"id":id,"ok":true}]}), i);
        }
        assert_eq!(store.enqueue(first.clone(), 80).unwrap(), "first");
        assert_eq!(store.status(80)["queued"], json!([]));
        first.op = Op::Destroy { target: "7f01".into() };
        assert!(store.enqueue(first, 80).is_err());
    }

    #[test]
    fn status_expires_offline_commands_and_zero_is_a_valid_full_timestamp() {
        let store = LiveStore::default();
        post(&store, report("b", 1, true), 0);
        assert_eq!(store.area_json("market", 10).unwrap()["fullAgeMs"], 10);
        store.enqueue(spawn(), 0).unwrap();
        assert_eq!(store.status(COMMAND_TTL_MS)["queued"], json!([]));
        assert_eq!(store.status(COMMAND_TTL_MS)["results"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn locating_a_transition_uses_the_newer_area_and_forgets_a_restart() {
        let store = LiveStore::default();
        post(
            &store,
            json!({"v":1,"boot":"a","seq":1,"areas":[{"area":"old","cre":[{"id":"1","x":2}]}]}),
            10,
        );
        post(
            &store,
            json!({"v":1,"boot":"a","seq":2,"areas":[{"area":"new","cre":[{"id":"1","x":8}]}]}),
            20,
        );
        let (area, kind, object) = store.locate("1").unwrap();
        assert_eq!((area.as_str(), kind, object.x), ("new", ObjectKind::Creature, 8.));
        post(&store, json!({"v":1,"boot":"b","seq":1}), 30);
        assert!(store.locate("1").is_none());
    }

    #[test]
    fn cheap_revisions_match_public_status_and_event_cursor() {
        let store = LiveStore::default();
        assert_eq!(store.revision(), (String::new(), 0));
        assert_eq!(store.event_cursor(), 0);
        for (boot, seq) in [("a", 1), ("a", 2), ("b", 1)] {
            post(
                &store,
                json!({"v":1,"boot":boot,"seq":seq,
                "events":[{"seq":seq,"kind":"chat","text":"hello"}]}),
                10,
            );
            assert_eq!(store.revision(), (boot.into(), seq));
            assert_eq!(store.event_cursor(), store.events(&crate::events::Query::default()).1);
            assert_eq!(store.status(10)["seq"], seq);
        }
        let cursor = store.event_cursor();
        store.server_log(
            &json!({"kind":"server_log","stream":"test","seq":1,"line":"between reports"}),
            11,
        );
        assert!(store.event_cursor() > cursor);
        assert_eq!(store.revision(), ("b".into(), 1));
    }
}
