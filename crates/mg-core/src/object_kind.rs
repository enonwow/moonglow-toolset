//! Kinds of objects placed in an area, shared without renderer dependencies.
use crate::ResType;

/// What an object in an area is: the GIT list it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectKind {
    Creature,
    Door,
    Encounter,
    Item,
    Placeable,
    Sound,
    Store,
    Trigger,
    Waypoint,
}

impl ObjectKind {
    pub const ALL: [ObjectKind; 9] = [
        ObjectKind::Creature,
        ObjectKind::Door,
        ObjectKind::Encounter,
        ObjectKind::Item,
        ObjectKind::Placeable,
        ObjectKind::Sound,
        ObjectKind::Store,
        ObjectKind::Trigger,
        ObjectKind::Waypoint,
    ];

    /// Its position in [`ObjectKind::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// What the toolset calls it (plural, as in its filters).
    pub fn plural(self) -> &'static str {
        match self {
            ObjectKind::Creature => "Creatures",
            ObjectKind::Door => "Doors",
            ObjectKind::Encounter => "Encounters",
            ObjectKind::Item => "Items",
            ObjectKind::Placeable => "Placeables",
            ObjectKind::Sound => "Sounds",
            ObjectKind::Store => "Merchants",
            ObjectKind::Trigger => "Triggers",
            ObjectKind::Waypoint => "Waypoints",
        }
    }

    /// The kind placed from a blueprint type.
    pub fn from_restype(t: ResType) -> Option<ObjectKind> {
        ObjectKind::ALL.into_iter().find(|k| k.restype() == t)
    }

    /// Its blueprint type.
    pub fn restype(self) -> ResType {
        match self {
            ObjectKind::Creature => ResType::UTC,
            ObjectKind::Door => ResType::UTD,
            ObjectKind::Encounter => ResType::UTE,
            ObjectKind::Item => ResType::UTI,
            ObjectKind::Placeable => ResType::UTP,
            ObjectKind::Sound => ResType::UTS,
            ObjectKind::Store => ResType::UTM,
            ObjectKind::Trigger => ResType::UTT,
            ObjectKind::Waypoint => ResType::UTW,
        }
    }

    /// The GIT list its instances are in.
    pub fn list(self) -> &'static str {
        match self {
            Self::Creature => "Creature List",
            Self::Door => "Door List",
            Self::Encounter => "Encounter List",
            Self::Item => "List",
            Self::Placeable => "Placeable List",
            Self::Sound => "SoundList",
            Self::Store => "StoreList",
            Self::Trigger => "TriggerList",
            Self::Waypoint => "WaypointList",
        }
    }

    /// Whether it is placed by `X`, `Y`, `Z` and `Bearing` (doors and
    /// placeables) rather than by position and orientation fields.
    pub fn has_bearing(self) -> bool {
        matches!(self, ObjectKind::Door | ObjectKind::Placeable)
    }

    /// Whether its model is the toolset's marker for it, not something
    /// the game shows: a waypoint's flag, a merchant's sign, a sound's
    /// speaker.
    pub fn is_marker(self) -> bool {
        matches!(self, ObjectKind::Waypoint | ObjectKind::Store | ObjectKind::Sound)
    }

    /// Whether it is drawn as an arrow along its facing when it has no
    /// marker model (or, a merchant, when arrows are preferred).
    pub fn has_arrow(self) -> bool {
        matches!(self, ObjectKind::Waypoint | ObjectKind::Store)
    }

    /// Whether it has an outline (`Geometry`) around its position.
    pub fn has_outline(self) -> bool {
        matches!(self, ObjectKind::Trigger | ObjectKind::Encounter)
    }
}
