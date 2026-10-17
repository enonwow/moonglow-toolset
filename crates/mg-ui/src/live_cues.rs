//! Short-lived event overlays. Receipt time limits their lifetime; opening a
//! view or changing server boot starts a new cursor instead of replaying a log.
use egui::{Color32, Stroke};
use glam::Vec3;
use mg_live::events::{Event, Query};
use mg_live::store::{AreaLive, LiveStore};

use crate::area_view::AreaView;
#[path = "live_cue_visuals.rs"]
mod visuals;

#[derive(Debug)]
struct PendingGround {
    id: String,
    data: serde_json::Value,
    queued: u64,
}

const MAX_CUES: usize = 128;
const FLASH_SECONDS: f64 = 0.4;
const SPEECH_SECONDS: f64 = 4.;

#[derive(Debug)]
struct Bubble {
    id: String,
    text: String,
    expires: f64,
}

impl Bubble {
    fn from_event(event: &Event, now_ms: u64, now: f64) -> Option<Self> {
        if event.kind != "chat" {
            return None;
        }
        let expires = now + SPEECH_SECONDS - now_ms.saturating_sub(event.at_ms) as f64 / 1000.;
        let mut chars = event.data["text"].as_str()?.chars();
        // Keep the complete message in Live Log; bound the overlay even when
        // a server sends very long lines. Whitespace must not create tall boxes.
        let mut text = chars.by_ref().take(240).collect::<String>();
        if chars.next().is_some() {
            text.push('…');
        }
        text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (expires > now && !text.is_empty())
            .then(|| Self { id: event.id.clone().unwrap_or_default(), text, expires })
            .filter(|b| !b.id.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    Spell,
    Projectile,
    Damage,
}

#[derive(Debug)]
struct Cue {
    key: String,
    visuals: visuals::Visuals,
    lasting: Option<f64>,
    transform: mg_area::VisualTransform,
    id: Option<String>,
    target: Option<String>,
    from: Option<Vec3>,
    to: Vec3,
    started: f64,
    received: u64,
    flight: f64,
    color: Color32,
    caption: String,
    spell: Option<i64>,
    origin: Origin,
    location_target: bool,
    radius: Option<f32>,
    labelled: bool,
    vfx: Option<i32>,
}

impl Cue {
    fn alive(&self, now: f64) -> bool {
        now - self.started < self.lasting.unwrap_or(self.flight + FLASH_SECONDS)
    }

    fn from_event(event: &Event, live: &AreaLive, now_ms: u64, now: f64) -> Option<Self> {
        let origin = match event.kind.as_str() {
            "spell" if event.data["countered"] != true => Origin::Spell,
            "vfx" if event.data["subtype"] == "projectile" => Origin::Projectile,
            "vfx" => Origin::Spell,
            "damage" => Origin::Damage,
            _ => return None,
        };
        let object = |id: &Option<String>| {
            live.find(id.as_deref()?).map(|(_, o)| o.position() + Vec3::Z * 0.7)
        };
        let position = &event.data["position"];
        let location = || {
            let p = Vec3::new(
                position["x"].as_f64()? as f32,
                position["y"].as_f64()? as f32,
                position["z"].as_f64()? as f32,
            );
            p.is_finite().then_some(p + Vec3::Z * 0.2)
        };
        // Native object projectiles sometimes supply a zero location vector.
        let target_position = object(&event.target);
        let location_target = target_position.is_none();
        let to = target_position.or_else(location)?;
        let from = object(&event.id);
        let flight = if origin == Origin::Damage
            || from.is_none()
            || (origin == Origin::Spell && event.data["instant"] == true)
        {
            0.
        } else if origin == Origin::Projectile {
            (event.data["delta_ms"].as_f64().unwrap_or(700.) / 1000.).clamp(0.5, 1.)
        } else {
            0.7
        };
        let color = if origin == Origin::Damage {
            Color32::LIGHT_RED
        } else {
            match event.data["projectile_type"].as_i64() {
                Some(1) => Color32::LIGHT_GREEN,
                Some(2) => Color32::LIGHT_BLUE,
                Some(3) => Color32::from_rgb(220, 200, 255),
                Some(4) => Color32::from_rgb(255, 145, 65),
                _ => Color32::from_rgb(255, 220, 120),
            }
        };
        let vector = |key: &str| {
            Vec3::new(
                event.data[key]["x"].as_f64().unwrap_or(0.) as f32,
                event.data[key]["y"].as_f64().unwrap_or(0.) as f32,
                event.data[key]["z"].as_f64().unwrap_or(0.) as f32,
            )
        };
        let cue = Self {
            key: format!("event-{}", event.n),
            visuals: Default::default(),
            lasting: match event.data["duration"].as_str() {
                Some("temporary") => event.data["seconds"]
                    .as_f64()
                    .filter(|s| s.is_finite() && *s > 0.)
                    .map(|s| s.min(86400.)),
                Some("permanent") => Some(f64::INFINITY),
                _ => None,
            },
            transform: mg_area::VisualTransform {
                scale: Vec3::splat(event.data["scale"].as_f64().unwrap_or(1.) as f32),
                rotate: vector("rotation"),
                translate: vector("offset"),
            },
            id: event.id.clone(),
            target: (!location_target).then(|| event.target.clone()).flatten(),
            from,
            to,
            started: now - now_ms.saturating_sub(event.at_ms) as f64 / 1000.,
            received: event.at_ms,
            flight,
            color,
            origin,
            location_target,
            radius: None,
            labelled: false,
            vfx: event.data["vfx"].as_i64().and_then(|n| i32::try_from(n).ok()),
            spell: event.data["spell"].as_i64().filter(|s| *s >= 0),
            caption: if origin == Origin::Damage {
                event.data["amount"].as_i64().map_or_else(String::new, |n| format!("-{n}"))
            } else {
                String::new()
            },
        };
        cue.alive(now).then_some(cue)
    }

    fn same_cast(&self, other: &Self) -> bool {
        matches!(
            (self.origin, other.origin),
            (Origin::Spell, Origin::Projectile) | (Origin::Projectile, Origin::Spell)
        ) && self.spell.is_some()
            && self.spell == other.spell
            && self.id == other.id
            && self.target == other.target
            && self.to.distance_squared(other.to) < 0.01
            && self.received.abs_diff(other.received) <= 250
    }
}

#[derive(Debug, Default)]
pub(crate) struct Cues {
    boot: Option<String>,
    last: u64,
    cues: Vec<Cue>,
    speech: Vec<Bubble>,
    pending: Vec<PendingGround>,
}

impl Cues {
    /// Resolve only newly received cues; no talk-table lookup during painting.
    pub(crate) fn resolve_names(&mut self, game: &mg_rules::GameData) {
        for cue in self.cues.iter_mut().filter(|c| !c.labelled) {
            cue.labelled = true;
            cue.visuals = visuals::Visuals::read(game, cue.spell, cue.vfx, cue.lasting.is_some());
            if cue.origin == Origin::Damage {
                continue;
            }
            if let Some(spell) = cue.spell {
                cue.caption = crate::live_effects::spell_name(game, spell);
                if cue.location_target {
                    cue.radius = crate::live_effects::spell_radius(game, spell);
                }
            } else if let Some(row) = cue.vfx {
                cue.caption = crate::live_effects::Effect::read(game, row).name;
            } else {
                cue.caption = "Visual effect".into();
            }
        }
    }
    pub(crate) fn advance(&mut self, live: &AreaLive, now: f64, effects: bool) {
        self.cues.retain(|cue| effects && cue.alive(now));
        self.speech.retain(|b| b.expires > now && live.find(&b.id).is_some());
    }

    pub(crate) fn update(
        &mut self,
        store: &LiveStore,
        area: &str,
        live: &AreaLive,
        now_ms: u64,
        now: f64,
        effects: bool,
    ) {
        self.advance(live, now, effects);
        let boot = store.boot();
        let (events, last) = store.events(&Query {
            since: self.last,
            area: Some(area.into()),
            kinds: vec!["spell".into(), "vfx".into(), "damage".into(), "chat".into()],
            ..Default::default()
        });
        let reset = self.boot.as_ref() != Some(&boot) || last < self.last;
        self.boot = Some(boot);
        self.last = last;

        if reset {
            self.cues.clear();
            self.speech.clear();
            self.pending.clear();
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        for pending in pending {
            if let Some(result) = store.result(&pending.id) {
                if effects && result.ok {
                    let event = Event {
                        n: 0,
                        at_ms: now_ms,
                        kind: "vfx".into(),
                        area: area.into(),
                        id: None,
                        target: None,
                        name: String::new(),
                        data: pending.data,
                    };
                    if let Some(mut cue) = Cue::from_event(&event, live, now_ms, now) {
                        cue.key = format!("command-{}", pending.id);
                        self.cues.push(cue);
                    }
                }
            } else if now_ms.saturating_sub(pending.queued) < 30_000 {
                self.pending.push(pending);
            }
        }
        for event in events {
            if let Some(bubble) = Bubble::from_event(&event, now_ms, now)
                && live.find(&bubble.id).is_some()
            {
                self.speech.retain(|old| old.id != bubble.id);
                self.speech.push(bubble);
            }
            if !effects {
                continue;
            }
            let Some(mut cue) = Cue::from_event(&event, live, now_ms, now) else { continue };
            if let Some(old) = self.cues.iter_mut().find(|old| old.same_cast(&cue)) {
                // Either wire event can arrive first. The explicit projectile
                // wins; retain the original start so duplication cannot extend it.
                if cue.origin == Origin::Projectile {
                    cue.started = cue.started.min(old.started);
                    *old = cue;
                }
            } else {
                self.cues.push(cue);
            }
        }
        let excess = self.cues.len().saturating_sub(MAX_CUES);
        self.cues.drain(..excess);
        let excess = self.speech.len().saturating_sub(MAX_CUES);
        self.speech.drain(..excess);
    }

    /// Ground commands currently have no native effect event. Draw only after
    /// their successful result, using the exact location the UI sent.
    pub(crate) fn ground_command(&mut self, id: String, op: &mg_live::store::Op) {
        let mg_live::store::Op::ApplyVfx { location: Some(_), .. } = op else { return };
        let Ok(mut data) = serde_json::to_value(op) else { return };
        data["vfx"] = data["row"].clone();
        data["position"] = data["location"].clone();
        self.pending.push(PendingGround { id, data, queued: mg_live::now_ms() });
        if self.pending.len() > MAX_CUES {
            self.pending.remove(0);
        }
    }
    pub(crate) fn has_models(&self, allowed: impl Fn(&str) -> bool) -> bool {
        self.cues.iter().any(|c| {
            c.id.iter().chain(&c.target).all(|id| allowed(id))
                && (c.visuals.cast.is_some()
                    || c.visuals.projectile.is_some()
                    || c.visuals.impact.is_some())
        })
    }
    pub(crate) fn stamp(&self) -> (u64, usize) {
        (self.last, self.cues.len())
    }
    pub(crate) fn objects(
        &self,
        now: f64,
        mut budget: usize,
        allowed: impl Fn(&str) -> bool,
    ) -> Vec<visuals::Draw> {
        let mut out = Vec::new();
        for cue in &self.cues {
            if cue.id.iter().chain(&cue.target).any(|id| !allowed(id)) {
                continue;
            }
            let elapsed = (now - cue.started).max(0.);
            let mut add =
                |stage: &str, model: &Option<mg_area::AreaObject>, position: Vec3, time: f64| {
                    let Some(mut object) = model.clone() else { return };
                    let count = object.preview.as_ref().map_or(0, |p| p.parts.len() + 1);
                    if count > budget {
                        return;
                    }
                    budget -= count;
                    object.position = position;
                    object.visual = Some(cue.transform);
                    out.push(visuals::Draw {
                        id: format!("{}-{stage}", cue.key),
                        object,
                        time: time as f32,
                    });
                };
            if elapsed < cue.flight {
                let from = cue.from.unwrap_or(cue.to);
                add("cast", &cue.visuals.cast, from, elapsed);
                let t = (elapsed / cue.flight) as f32;
                let height = (from.distance(cue.to) * 0.2).clamp(0.5, 3.);
                let at = from.lerp(cue.to, t) + Vec3::Z * (4. * t * (1. - t) * height);
                add("projectile", &cue.visuals.projectile, at, elapsed);
            } else {
                add("impact", &cue.visuals.impact, cue.to, elapsed - cue.flight);
            }
        }
        out
    }

    pub(crate) fn len(&self) -> usize {
        self.cues.len()
    }

    pub(crate) fn speech_len(&self) -> usize {
        self.speech.len()
    }

    pub(crate) fn paint_speech(
        &self,
        ui: &egui::Ui,
        view: &AreaView,
        now: f64,
        reserved: &mut Vec<egui::Rect>,
        position: impl Fn(&str) -> Option<egui::Pos2>,
    ) {
        let painter = ui.painter_at(view.rect);
        let visible = view.rect.intersect(painter.clip_rect()).shrink(4.);
        let mut bubbles: Vec<_> = self.speech.iter().collect();
        bubbles.sort_by_key(|b| view.live.selected.as_deref() != Some(b.id.as_str()));
        for bubble in bubbles {
            // This is a static overlay: repaint at expiry, not every 3D frame.
            crate::live_repaint::after(
                ui.ctx(),
                std::time::Duration::from_secs_f64((bubble.expires - now).max(0.)),
            );
            let Some(at) = position(&bubble.id).filter(|at| visible.contains(*at)) else {
                continue;
            };
            let galley = painter.layout(
                bubble.text.clone(),
                egui::FontId::proportional(14.),
                Color32::WHITE,
                220.,
            );
            let size = galley.size() + egui::vec2(16., 12.);
            // Try increasingly higher positions, retaining room for the name
            // beneath the bubble and avoiding other markers, bubbles and legend.
            let rect = (0..4)
                .map(|i| {
                    let x = (at.x - size.x * 0.5).max(visible.left()).min(visible.right() - size.x);
                    egui::Rect::from_min_size(
                        egui::pos2(x, at.y - 42. - size.y - i as f32 * (size.y + 8.)),
                        size,
                    )
                })
                .find(|rect| {
                    visible.contains_rect(*rect)
                        && !reserved.iter().any(|r| r.expand(3.).intersects(*rect))
                });
            let Some(rect) = rect else { continue };
            let color = Color32::from_rgb(34, 39, 49);
            let tip_x = at.x.clamp(rect.left() + 6., rect.right() - 6.);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(tip_x - 5., rect.bottom() - 1.),
                    egui::pos2(tip_x + 5., rect.bottom() - 1.),
                    egui::pos2(tip_x, rect.bottom() + 7.),
                ],
                color,
                Stroke::NONE,
            ));
            painter.rect_filled(rect, 5., color);
            painter.rect_stroke(rect, 5., Stroke::new(1., Color32::GRAY), egui::StrokeKind::Inside);
            painter.galley(rect.min + egui::vec2(8., 6.), galley, Color32::WHITE);
            reserved.push(rect.expand(7.));
        }
    }

    pub(crate) fn paint(
        &self,
        ui: &egui::Ui,
        view: &AreaView,
        now: f64,
        allowed: impl Fn(&str) -> bool,
    ) {
        let painter = ui.painter_at(view.rect);
        for cue in &self.cues {
            if cue.id.iter().chain(&cue.target).any(|id| !allowed(id)) {
                continue;
            }
            let elapsed = (now - cue.started).max(0.);
            if let Some(duration) = cue.lasting {
                if let Some(at) = view.screen_pos(cue.to) {
                    painter.circle_stroke(at, 15., Stroke::new(2., cue.color));
                    painter.text(
                        at - egui::vec2(0., 20.),
                        egui::Align2::CENTER_BOTTOM,
                        &cue.caption,
                        egui::FontId::proportional(14.),
                        cue.color,
                    );
                }
                if duration.is_finite() {
                    crate::live_repaint::after(
                        ui.ctx(),
                        std::time::Duration::from_secs_f64((duration - elapsed).max(0.)),
                    );
                }
                continue;
            }
            if let Some(radius) = cue.radius {
                let points: Vec<_> = (0..=64)
                    .filter_map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / 64.;
                        view.screen_pos(cue.to + Vec3::new(angle.cos(), angle.sin(), 0.) * radius)
                    })
                    .collect();
                let top = points.iter().copied().min_by(|a, b| a.y.total_cmp(&b.y));
                let opacity = ((cue.flight + FLASH_SECONDS - elapsed) * 4.).clamp(0., 1.) as f32;
                let color = cue.color.gamma_multiply(opacity);
                painter.add(egui::Shape::line(points, Stroke::new(2., color)));
                if let Some(top) = top {
                    painter.text(
                        top - egui::vec2(0., 8.),
                        egui::Align2::CENTER_BOTTOM,
                        format!("{} · {radius:.2} m", cue.caption),
                        egui::FontId::proportional(15.),
                        color,
                    );
                }
            }
            if elapsed < cue.flight {
                let from = cue.from.unwrap_or(cue.to);
                let height = (from.distance(cue.to) * 0.2).clamp(0.5, 3.);
                let arc = |t: f32| from.lerp(cue.to, t) + Vec3::Z * (4. * t * (1. - t) * height);
                let points =
                    (0..=24).filter_map(|i| view.screen_pos(arc(i as f32 / 24.))).collect();
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(1.5, cue.color.gamma_multiply(0.5)),
                ));
                if let Some(at) = view.screen_pos(arc((elapsed / cue.flight) as f32)) {
                    painter.circle_filled(at, 5., cue.color);
                    painter.circle_stroke(at, 9., Stroke::new(1.5, cue.color));
                    if cue.radius.is_none() && !cue.caption.is_empty() {
                        painter.text(
                            at - egui::vec2(0., 14.),
                            egui::Align2::CENTER_BOTTOM,
                            &cue.caption,
                            egui::FontId::proportional(14.),
                            cue.color,
                        );
                    }
                }
            } else if let Some(at) = view.screen_pos(cue.to) {
                let t = ((elapsed - cue.flight) / FLASH_SECONDS).clamp(0., 1.) as f32;
                // Keep a brief event legible, then fade during its last 100 ms.
                let color = cue.color.gamma_multiply(((1. - t) * 4.).min(1.));
                painter.circle_filled(at, 8. * (1. - t), color);
                painter.circle_stroke(at, 9. + t * 24., Stroke::new(2.5, color));
                if !cue.caption.is_empty() && cue.radius.is_none() {
                    painter.text(
                        at + egui::vec2(0., -30. - t * 12.),
                        egui::Align2::CENTER_BOTTOM,
                        &cue.caption,
                        egui::FontId::proportional(17.),
                        color,
                    );
                }
            }
        }
        if self.cues.iter().any(|c| c.lasting.is_none()) {
            crate::live_repaint::after(ui.ctx(), std::time::Duration::from_millis(40));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> LiveStore {
        let store = LiveStore::default();
        store
            .sync(
                json!({"v":1,"boot":"a","seq":1,"areas":[{"area":"a","full":true,
            "cre":[{"id":"1","x":10,"y":10},{"id":"2","x":15,"y":10}]}]})
                .to_string()
                .as_bytes(),
                1000,
            )
            .unwrap();
        store
    }

    #[test]
    fn ground_visual_waits_for_success_and_expires_or_clears_on_boot() {
        let store = store();
        let live = store.area("a").unwrap();
        let mut cues = Cues::default();
        cues.update(&store, "a", &live, 1000, 0., true);
        let op: mg_live::store::Op = serde_json::from_value(json!({
            "op":"apply_vfx","row":22,"location":{"x":4,"y":6,"z":0},"duration":"instant"
        }))
        .unwrap();
        let id = store
            .enqueue(
                mg_live::store::Command { id: String::new(), area: "a".into(), op: op.clone() },
                1000,
            )
            .unwrap();
        cues.ground_command(id.clone(), &op);
        cues.update(&store, "a", &live, 1000, 0., true);
        assert_eq!(cues.len(), 0, "queued is not applied");
        store
            .sync(
                json!({"v":1,"boot":"a","seq":2,"areas":[],"results":[{"id":id,"ok":true}]})
                    .to_string()
                    .as_bytes(),
                1100,
            )
            .unwrap();
        cues.update(&store, "a", &live, 1100, 0.1, true);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues.cues[0].vfx, Some(22));
        assert_eq!(cues.cues[0].to, Vec3::new(4., 6., 0.2));
        cues.advance(&live, 0.51, true);
        assert_eq!(cues.len(), 0);
        let id = store
            .enqueue(
                mg_live::store::Command { id: String::new(), area: "a".into(), op: op.clone() },
                1200,
            )
            .unwrap();
        cues.ground_command(id.clone(), &op);
        store.sync(json!({"v":1,"boot":"a","seq":3,"areas":[],"results":[{"id":id,"ok":false,"err":"Refused"}]}).to_string().as_bytes(),1200).unwrap();
        cues.update(&store, "a", &live, 1200, 0.6, true);
        assert_eq!(cues.len(), 0, "refusal does not display a successful effect");
        cues.ground_command("unacknowledged".into(), &op);
        store
            .sync(json!({"v":1,"boot":"new","seq":1,"areas":[]}).to_string().as_bytes(), 1300)
            .unwrap();
        cues.update(&store, "a", &live, 1300, 0.7, true);
        assert!(cues.pending.is_empty());
    }

    #[test]
    fn speech_replaces_per_speaker_preserves_unicode_and_expires_with_effects_off() {
        let store = store();
        let live = store.area("a").unwrap();
        let mut cues = Cues::default();
        cues.update(&store, "a", &live, 1000, 0., false);
        let report = |seq, text| {
            json!({"v":1,"boot":"a","seq":seq,"areas":[],"events":[
                {"seq":seq,"kind":"chat","area":"a","id":"1","text":text,"channel":1}
            ]})
        };
        store.sync(report(2, "Za mną!\n  Pilnujcie bramy.").to_string().as_bytes(), 1000).unwrap();
        // A half-second spent waiting for this view does not extend the lifetime.
        cues.update(&store, "a", &live, 1500, 0.5, false);
        assert_eq!(cues.speech_len(), 1);
        assert_eq!(cues.speech[0].text, "Za mną! Pilnujcie bramy.");
        assert_eq!(cues.speech[0].expires, 4.);
        let long = "ż".repeat(300);
        store.sync(report(3, &long).to_string().as_bytes(), 2500).unwrap();
        cues.update(&store, "a", &live, 2500, 1.5, false);
        assert_eq!(cues.speech_len(), 1);
        assert_eq!(cues.speech[0].text, format!("{}…", "ż".repeat(240)));
        cues.update(&store, "a", &live, 6000, 5., false);
        assert_eq!(cues.speech_len(), 1);
        cues.update(&store, "a", &live, 6500, 5.5, false);
        assert_eq!(cues.speech_len(), 0);
        store.sync(report(4, "Stale").to_string().as_bytes(), 2500).unwrap();
        cues.update(&store, "a", &live, 7000, 6., true);
        assert_eq!(cues.speech_len(), 0);
        store.sync(report(5, "New").to_string().as_bytes(), 7000).unwrap();
        cues.update(&store, "a", &live, 7000, 6., true);
        assert_eq!(cues.speech_len(), 1);
        store
            .sync(json!({"v":1,"boot":"b","seq":1,"areas":[]}).to_string().as_bytes(), 7000)
            .unwrap();
        cues.update(&store, "a", &live, 7000, 6.1, true);
        assert_eq!(cues.speech_len(), 0);
    }

    #[test]
    fn cursor_deduplicates_casts_uses_object_position_and_expires() {
        let store = store();
        let live = store.area("a").unwrap();
        let mut cues = Cues::default();
        cues.update(&store, "a", &live, 1000, 0., true);
        store.sync(json!({"v":1,"boot":"a","seq":2,"areas":[],"events":[
            {"seq":1,"kind":"spell","area":"a","id":"1","target":"2","spell":107,"instant":false},
            {"seq":2,"kind":"vfx","subtype":"projectile","area":"a","id":"1","target":"2","spell":107,"delta_ms":0,"position":{"x":0,"y":0,"z":0}}
        ]}).to_string().as_bytes(),1000).unwrap();
        cues.update(&store, "a", &live, 1000, 0., true);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues.cues[0].to, Vec3::new(15., 10., 0.7));
        assert_eq!(cues.cues[0].flight, 0.5);
        cues.update(&store, "a", &live, 1000, 0.1, true);
        assert_eq!(cues.len(), 1);
        cues.update(&store, "a", &live, 2000, 1., true);
        assert_eq!(cues.len(), 0);
    }

    #[test]
    fn location_damage_off_and_boot_reset_have_bounded_lifetimes() {
        let store = store();
        let live = store.area("a").unwrap();
        let mut cues = Cues::default();
        cues.update(&store, "a", &live, 1000, 0., true);
        store
            .sync(
                json!({"v":1,"boot":"a","seq":2,"areas":[],"events":[
                    {"seq":1,"kind":"spell","area":"a","id":"1","position":{"x":20,"y":21,"z":0}},
                    {"seq":2,"kind":"damage","area":"a","id":"1","target":"2","amount":7}
                ]})
                .to_string()
                .as_bytes(),
                1000,
            )
            .unwrap();
        cues.update(&store, "a", &live, 1000, 0., true);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues.cues[0].to, Vec3::new(20., 21., 0.2));
        assert_eq!(cues.cues[1].caption, "-7");
        cues.update(&store, "a", &live, 1000, 0.1, false);
        assert_eq!(cues.len(), 0);
        cues.update(&store, "a", &live, 1000, 0.2, true);
        assert_eq!(cues.len(), 0);
        store
            .sync(
                json!({"v":1,"boot":"b","seq":1,"areas":[],"events":[
                    {"seq":1,"kind":"damage","area":"a","id":"1","target":"2","amount":9}
                ]})
                .to_string()
                .as_bytes(),
                1000,
            )
            .unwrap();
        cues.update(&store, "a", &live, 1000, 0.3, true);
        assert_eq!(cues.len(), 0);
    }
}
