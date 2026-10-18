//! Parse and validate a complete wire report before the store lock is taken.
use super::{
    HashSet, MAX_AREAS, MAX_EVENTS, MAX_OBJECTS, MAX_QUEUE, MAX_TEXT, SyncRequest, Value, finite,
    object_id, resref, short, validate_object_json,
};

impl SyncRequest {
    pub(super) fn parse(body: &[u8]) -> Result<Self, String> {
        // The game budgets result batches to 512 KiB in addition to the
        // existing 4 MiB state allowance. Keep headroom for JSON framing.
        if body.len() > 5 * 1024 * 1024 {
            return Err("Body too large".into());
        }
        let mut message: Value =
            serde_json::from_slice(body).map_err(|e| format!("Bad sync request: {e}"))?;
        crate::wire_text::report(&mut message);
        let request: SyncRequest =
            serde_json::from_value(message).map_err(|e| format!("Bad sync request: {e}"))?;
        if request.v != 1 {
            return Err(format!("Unsupported protocol version {}", request.v));
        }
        if request.boot.is_empty() {
            return Err("boot is required".into());
        }
        short("boot", &request.boot, 64)?;
        if let Some(areas) = &request.watched_areas {
            if areas.len() > 4096 {
                return Err("Too many watched areas".into());
            }
            for area in areas {
                resref(area)?;
            }
        }
        if let Some(factions) = &request.factions {
            if factions.len() > 65536 {
                return Err("Too many factions".into());
            }
            let mut seen = HashSet::new();
            for faction in factions {
                if faction.id > 65535 || !seen.insert(faction.id) {
                    return Err("Invalid or repeated faction id".into());
                }
                short("faction name", &faction.name, 256)?;
            }
        }
        if request.ack.len() > MAX_QUEUE || request.results.len() > MAX_QUEUE {
            return Err("Too many acknowledgements or results".into());
        }
        for id in &request.ack {
            short("ack id", id, 64)?;
        }
        for r in &request.results {
            short("result id", &r.id, 64)?;
            if let Some(by) = &r.by {
                short("result by", by, 64)?;
            }
            // Engine compiler diagnostics can be substantially longer than names.
            short("result error", &r.err, 16 * 1024)?;
            short("result object", &r.obj, 16)?;
            if r.value.as_ref().is_some_and(|v| crate::bridge::game_json(v).len() > 16384) {
                return Err("Result value exceeds 16384 JSON bytes".into());
            }
            if let Some(snapshot) = &r.object_json {
                validate_object_json(snapshot)?;
            }
            if let Some(faction) = &r.faction {
                if let Some(note) = faction.as_str() {
                    short("result faction", note, 32)?;
                } else if let Some(pair) = faction.as_object() {
                    if pair.len() != 2
                        || !["original", "temporary"].iter().all(|key| {
                            pair.get(*key).and_then(Value::as_u64).is_some_and(|n| n <= 65535)
                        })
                    {
                        return Err("Invalid fighting faction result".into());
                    }
                } else if !faction.as_u64().is_some_and(|n| n <= 65535) {
                    return Err("Invalid result faction".into());
                }
            }
            if r.objects.len() > 10
                || r.count.is_some_and(|n| n > 10)
                || r.affected.len() > MAX_OBJECTS
            {
                return Err("Too many result objects".into());
            }
            for id in r.objects.iter().chain(&r.affected) {
                object_id("result object", id)?;
            }
        }
        if request.events.len() > MAX_EVENTS {
            return Err(format!("More than {MAX_EVENTS} events"));
        }
        if request.areas.len() > MAX_AREAS {
            return Err(format!("More than {MAX_AREAS} areas"));
        }
        let mut area_names = HashSet::new();
        for a in &request.areas {
            resref(&a.area)?;
            if !area_names.insert(a.area.to_ascii_lowercase()) {
                return Err("Duplicate area".into());
            }
            if a.removed.len() > MAX_OBJECTS * 9 {
                return Err("Too many removed IDs".into());
            }
            for id in &a.removed {
                object_id("removed id", id)?;
            }
            let mut ids = HashSet::new();
            for o in a
                .cre
                .iter()
                .chain(&a.plc)
                .chain(&a.doors)
                .chain(&a.items)
                .chain(&a.waypoints)
                .chain(&a.sounds)
                .chain(&a.encounters)
                .chain(&a.stores)
                .chain(&a.triggers)
            {
                if !ids.insert(&o.id) {
                    return Err("Duplicate object id".into());
                }
            }
            for list in [
                &a.cre,
                &a.plc,
                &a.doors,
                &a.items,
                &a.waypoints,
                &a.sounds,
                &a.encounters,
                &a.stores,
                &a.triggers,
            ] {
                if list.len() > MAX_OBJECTS {
                    return Err(format!("Area {} reports more than {MAX_OBJECTS} objects", a.area));
                }
                for o in list {
                    object_id("object id", &o.id)?;
                    finite(&[o.x, o.y, o.z, o.facing])?;
                    if o.vfx.len() > 16 {
                        return Err("More than 16 lasting VFX rows".into());
                    }
                    if let Some(vt) = &o.vt {
                        finite(&[
                            vt.scale,
                            vt.rotate.x,
                            vt.rotate.y,
                            vt.rotate.z,
                            vt.translate.x,
                            vt.translate.y,
                            vt.translate.z,
                        ])?;
                    }
                    if let Some(target) = &o.target {
                        object_id("attack target", target)?;
                    }
                    if o.hostile.len() > MAX_OBJECTS {
                        return Err("Too many hostile PCs".into());
                    }
                    for pc in &o.hostile {
                        object_id("hostile PC", pc)?;
                    }
                    if let Some(friendly) = &o.friendly {
                        if friendly.len() > MAX_OBJECTS {
                            return Err("Too many friendly PCs".into());
                        }
                        for pc in friendly {
                            object_id("friendly PC", pc)?;
                        }
                    }
                    if let Some(look) = &o.look
                        && serde_json::to_vec(look).map_err(|e| e.to_string())?.len() > 64 * 1024
                    {
                        return Err("Player look exceeds 64 KiB".into());
                    }
                    for s in [&o.id, &o.tag, &o.resref, &o.name] {
                        short("object field", s, MAX_TEXT)?;
                    }
                }
            }
        }
        Ok(request)
    }
}
