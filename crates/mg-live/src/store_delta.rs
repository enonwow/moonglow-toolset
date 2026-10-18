//! Validate retained sizes before changing any area, then move rows once.
use super::{AreaLive, AreaReport, HashMap, HashSet, LiveObject, MAX_OBJECTS};

enum Row {
    Old(usize),
    New(usize, Option<usize>),
}

pub(super) struct DeltaPlan {
    rows: [Vec<Row>; 9],
    pub(super) scan: Option<super::StaticScan>,
}

impl DeltaPlan {
    pub(super) fn new(old: &AreaLive, update: &AreaReport) -> Result<Self, String> {
        Self::build(old, update, None)
    }

    pub(super) fn paged(
        old: &AreaLive,
        update: &AreaReport,
        previous: &super::StaticScan,
    ) -> Result<Self, String> {
        Self::build(old, update, Some(super::StaticScan::advance(previous, update)?))
    }

    fn build(
        old: &AreaLive,
        update: &AreaReport,
        scan: Option<super::StaticScan>,
    ) -> Result<Self, String> {
        let changed: HashSet<&str> = update
            .cre
            .iter()
            .chain(&update.plc)
            .chain(&update.doors)
            .chain(&update.items)
            .chain(&update.waypoints)
            .chain(&update.sounds)
            .chain(&update.encounters)
            .chain(&update.stores)
            .chain(&update.triggers)
            .map(|o| o.id.as_str())
            .collect();
        let removed: HashSet<&str> = update.removed.iter().map(String::as_str).collect();
        let before = [
            &old.creatures,
            &old.placeables,
            &old.doors,
            &old.items,
            &old.waypoints,
            &old.sounds,
            &old.encounters,
            &old.stores,
            &old.triggers,
        ];
        let after = [
            &update.cre,
            &update.plc,
            &update.doors,
            &update.items,
            &update.waypoints,
            &update.sounds,
            &update.encounters,
            &update.stores,
            &update.triggers,
        ];
        let rows = std::array::from_fn(|kind| {
            let mut pending: HashMap<&str, usize> =
                after[kind].iter().enumerate().map(|(i, o)| (o.id.as_str(), i)).collect();
            let mut rows = Vec::new();
            for (i, old) in before[kind].iter().enumerate() {
                if let Some(j) = pending.remove(old.id.as_str()) {
                    rows.push(Row::New(j, Some(i)));
                } else if !removed.contains(old.id.as_str()) && !changed.contains(old.id.as_str()) {
                    let keep = match &scan {
                        None => true,
                        Some(_) if kind == 0 => update.cre_partial,
                        Some(scan) => !scan.complete || scan.seen.contains(&old.id),
                    };
                    if keep {
                        rows.push(Row::Old(i));
                    }
                }
            }
            for (i, new) in after[kind].iter().enumerate() {
                if pending.remove(new.id.as_str()).is_some() {
                    rows.push(Row::New(i, None));
                }
            }
            rows
        });
        if rows.iter().any(|v| v.len() > MAX_OBJECTS) {
            return Err("Too many retained delta objects".into());
        }
        // Static page accumulation must remain bounded even across many reports.
        if scan.is_some() {
            let mut bytes = 0usize;
            for kind in 1..9 {
                for row in &rows[kind] {
                    let object = match *row {
                        Row::Old(i) => &before[kind][i],
                        Row::New(i, _) => &after[kind][i],
                    };
                    bytes += serde_json::to_vec(object).map_err(|e| e.to_string())?.len();
                    if bytes > 8 * 1024 * 1024 {
                        return Err("Too many retained static page bytes".into());
                    }
                }
            }
        }
        Ok(Self { rows, scan })
    }

    pub(super) fn creatures<'a>(
        &'a self,
        old: &'a AreaLive,
        update: &'a AreaReport,
    ) -> impl Iterator<Item = &'a LiveObject> + Clone {
        self.rows[0].iter().map(|row| match *row {
            Row::Old(i) => &old.creatures[i],
            Row::New(i, _) => &update.cre[i],
        })
    }

    pub(super) fn apply(self, old: &mut AreaLive, update: &mut AreaReport) {
        let reuse_player_look = update.paged && !update.full;
        let before = [
            &mut old.creatures,
            &mut old.placeables,
            &mut old.doors,
            &mut old.items,
            &mut old.waypoints,
            &mut old.sounds,
            &mut old.encounters,
            &mut old.stores,
            &mut old.triggers,
        ];
        let after = [
            &mut update.cre,
            &mut update.plc,
            &mut update.doors,
            &mut update.items,
            &mut update.waypoints,
            &mut update.sounds,
            &mut update.encounters,
            &mut update.stores,
            &mut update.triggers,
        ];
        for ((rows, before), after) in self.rows.into_iter().zip(before).zip(after) {
            let mut old_rows: Vec<_> = std::mem::take(before).into_iter().map(Some).collect();
            let mut new_rows: Vec<_> = std::mem::take(after).into_iter().map(Some).collect();
            *before = rows
                .into_iter()
                .map(|row| match row {
                    Row::Old(i) => old_rows[i].take().expect("each row is moved once"),
                    Row::New(i, previous) => {
                        let mut new = new_rows[i].take().expect("each row is moved once");
                        if new.look.is_none()
                            && !new.look_pending
                            && let Some(old) = previous.and_then(|j| old_rows[j].as_mut())
                            && ((new.look_hash.is_some() && new.look_hash == old.look_hash)
                                || (reuse_player_look && new.pc))
                        {
                            new.look = old.look.take();
                            new.look_hash = old.look_hash;
                        }
                        new
                    }
                })
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn world_kinds_survive_delta_merge_and_are_removed_and_findable() {
        let store = super::super::LiveStore::default();
        let kinds = ["waypoints", "sounds", "encounters", "stores", "triggers"];
        let mut area = json!({"area":"room","full":true});
        for (i, kind) in kinds.iter().enumerate() {
            area[kind] = json!([{"id":format!("{:x}",i+1),"name":kind,"x":5,"y":5}]);
        }
        store
            .sync(json!({"v":1,"boot":"b","seq":1,"areas":[area]}).to_string().as_bytes(), 1000)
            .unwrap();
        assert_eq!(store.area("room").unwrap().all().count(), 5);
        assert_eq!(store.find("", 10).len(), 5);
        store.sync(br#"{"v":1,"boot":"b","seq":2,"base_seq":1,"areas":[{"area":"room","sounds":[{"id":"2","x":7}],"removed":["3"]}]}"#,1001).unwrap();
        let a = store.area("room").unwrap();
        assert_eq!(a.all().count(), 4);
        assert_eq!(a.sounds[0].x, 7.0);
        assert!(a.encounters.is_empty());
        for kind in kinds {
            assert!(store.area_json("room", 1001).unwrap().get(kind).unwrap().is_array());
        }
        let command: super::super::Command =
            serde_json::from_value(json!({"area":"room","op":"destroy","target":"2"})).unwrap();
        store.enqueue(command, 1001).unwrap();
        for kind in ["waypoint", "sound", "encounter", "store", "trigger"] {
            let command:super::super::Command=serde_json::from_value(json!({"area":"room","op":"spawn","kind":kind,"r":"test","x":5,"y":5,"z":0,"f":0,"count":2})).unwrap();
            command.validate().unwrap();
            assert_eq!(serde_json::to_value(command).unwrap()["kind"], kind);
        }
        for shape in [
            json!({"shape":"circle","radius":31}),
            json!({"shape":"rectangle","width":2}),
            json!({"shape":"circle","radius":2,"width":3}),
        ] {
            let mut command = json!({"area":"room","op":"spawn_trigger","x":5,"y":5,"z":0,"f":0});
            command.as_object_mut().unwrap().extend(shape.as_object().unwrap().clone());
            assert!(
                serde_json::from_value::<super::super::Command>(command)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
    }

    #[test]
    fn moving_row_reuses_only_an_exact_look_hash_without_cloning() {
        for hash in [Some(12), Some(13), None] {
            let mut old = AreaLive {
                creatures: vec![LiveObject {
                    id: "1".into(),
                    look_hash: Some(12),
                    look: Some(json!({"parts": "x".repeat(32000)})),
                    ..Default::default()
                }],
                ..Default::default()
            };
            let ptr = old.creatures[0].look.as_ref().unwrap()["parts"].as_str().unwrap().as_ptr();
            let mut update: AreaReport = serde_json::from_value(json!({
                "area":"a", "cre":[{"id":"1","x":42,"lookHash":hash}]
            }))
            .unwrap();
            DeltaPlan::new(&old, &update).unwrap().apply(&mut old, &mut update);
            assert_eq!(old.creatures[0].x, 42.0);
            if hash == Some(12) {
                assert_eq!(
                    old.creatures[0].look.as_ref().unwrap()["parts"].as_str().unwrap().as_ptr(),
                    ptr
                );
            } else {
                assert!(old.creatures[0].look.is_none(), "never reuse a different appearance");
            }
        }
    }

    #[test]
    fn planned_merge_matches_reference_and_keeps_unchanged_look_allocation() {
        let object = |id: &str| LiveObject {
            id: id.into(),
            look: Some(json!({"large": "x".repeat(32000)})),
            ..Default::default()
        };
        let before = AreaLive {
            creatures: vec![object("1"), object("2"), object("3"), object("4")],
            placeables: vec![object("5")],
            ..Default::default()
        };
        for removed in [vec![], vec!["1", "2", "5"]] {
            let mut update: AreaReport = serde_json::from_value(json!({"area":"a", "cre":[{"id":"1","name":"new"},{"id":"6"}], "items":[{"id":"2"}], "removed":removed})).unwrap();
            let expected = merge_delta(&before, &update);
            let mut actual = before.clone();
            let ptr =
                actual.creatures[2].look.as_ref().unwrap()["large"].as_str().unwrap().as_ptr();
            let plan = DeltaPlan::new(&actual, &update).unwrap();
            assert_eq!(
                plan.creatures(&actual, &update).cloned().collect::<Vec<_>>(),
                expected.creatures
            );
            plan.apply(&mut actual, &mut update);
            assert_eq!(actual.creatures, expected.creatures);
            assert_eq!(actual.placeables, expected.placeables);
            assert_eq!(actual.items, expected.items);
            let retained = actual.creatures.iter().find(|o| o.id == "3").unwrap();
            assert_eq!(retained.look.as_ref().unwrap()["large"].as_str().unwrap().as_ptr(), ptr);
        }
    }

    // Frozen pre-optimization merge, used only as an independent test oracle.
    fn merge_delta(old: &AreaLive, update: &AreaReport) -> AreaLive {
        let changed: HashSet<&str> = update
            .cre
            .iter()
            .chain(&update.plc)
            .chain(&update.doors)
            .chain(&update.items)
            .chain(&update.waypoints)
            .chain(&update.sounds)
            .chain(&update.encounters)
            .chain(&update.stores)
            .chain(&update.triggers)
            .map(|o| o.id.as_str())
            .collect();
        let removed: HashSet<&str> = update.removed.iter().map(String::as_str).collect();
        let merge = |before: &[LiveObject], after: &[LiveObject]| {
            let mut updates: HashMap<&str, &LiveObject> =
                after.iter().map(|o| (o.id.as_str(), o)).collect();
            let mut rows = Vec::new();
            for old in before {
                if let Some(new) = updates.remove(old.id.as_str()) {
                    rows.push(new.clone());
                } else if !removed.contains(old.id.as_str()) && !changed.contains(old.id.as_str()) {
                    rows.push(old.clone());
                }
            }
            for new in after {
                if updates.remove(new.id.as_str()).is_some() {
                    rows.push(new.clone());
                }
            }
            rows
        };
        AreaLive {
            creatures_partial: old.creatures_partial,
            static_observed: old.static_observed,
            size: old.size,
            seq: old.seq,
            received_ms: old.received_ms,
            full_ms: old.full_ms,
            has_full: old.has_full,
            truncated: old.truncated,
            omitted_at_least: old.omitted_at_least,
            creatures: merge(&old.creatures, &update.cre),
            placeables: merge(&old.placeables, &update.plc),
            doors: merge(&old.doors, &update.doors),
            items: merge(&old.items, &update.items),
            waypoints: merge(&old.waypoints, &update.waypoints),
            sounds: merge(&old.sounds, &update.sounds),
            encounters: merge(&old.encounters, &update.encounters),
            stores: merge(&old.stores, &update.stores),
            triggers: merge(&old.triggers, &update.triggers),
        }
    }
}
