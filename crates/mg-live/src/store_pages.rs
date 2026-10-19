//! Independent static scan generations. A missing page never proves deletion.
use super::{AreaReport, HashMap, HashSet, MAX_OBJECTS};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct StaticPage {
    pub generation: u64,
    pub index: u32,
    pub complete: bool,
}

/// Host-side scan bookkeeping; mirrors only need the exported coverage flags.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct StaticScan {
    generation: u64,
    next: u32,
    valid: bool,
    pub(super) complete: bool,
    pub(super) seen: HashSet<String>,
    /// Consecutive partial creature lists that omitted each retained creature.
    pub(super) missed: HashMap<String, u8>,
}

impl StaticScan {
    pub(super) fn advance(old: &Self, update: &AreaReport) -> Result<Self, String> {
        let Some(page) = &update.static_page else {
            return Ok(old.clone());
        };
        let mut scan = if page.index == 0 || page.generation != old.generation {
            Self { generation: page.generation, valid: page.index == 0, ..Self::default() }
        } else {
            old.clone()
        };
        scan.missed.clone_from(&old.missed);
        scan.valid &= scan.generation == page.generation && scan.next == page.index;
        scan.next = page.index.checked_add(1).ok_or("Static page index overflow")?;
        scan.complete = page.complete && scan.valid;
        for o in update
            .plc
            .iter()
            .chain(&update.doors)
            .chain(&update.items)
            .chain(&update.waypoints)
            .chain(&update.sounds)
            .chain(&update.encounters)
            .chain(&update.stores)
            .chain(&update.triggers)
        {
            scan.seen.insert(o.id.clone());
            if scan.seen.len() > MAX_OBJECTS * 8 {
                return Err("Too many static scan IDs".into());
            }
        }
        Ok(scan)
    }
}
