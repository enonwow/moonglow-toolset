//! Report-paced motion. The clock is egui's monotonic time, not the wall clock.
use crate::live::{AreaLive, LiveObject};
use glam::Vec3;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy)]
struct Pose {
    position: Vec3,
    facing: f32,
}
impl From<&LiveObject> for Pose {
    fn from(o: &LiveObject) -> Self {
        Self { position: o.position(), facing: o.facing }
    }
}

fn turn(from: f32, to: f32) -> f32 {
    (to - from + 180.).rem_euclid(360.) - 180.
}

#[derive(Debug)]
struct Track {
    from: Pose,
    to: Pose,
    at: f64,
    duration: f64,
    trail: VecDeque<(f64, Vec3)>,
}
impl Track {
    fn fraction(&self, now: f64) -> f32 {
        if self.duration <= 0. {
            1.
        } else {
            ((now - self.at) / self.duration).clamp(0., 1.) as f32
        }
    }
    fn pose(&self, now: f64) -> Pose {
        let t = self.fraction(now);
        Pose {
            position: self.from.position.lerp(self.to.position, t),
            facing: (self.from.facing + turn(self.from.facing, self.to.facing) * t)
                .rem_euclid(360.),
        }
    }
    fn moving(&self, now: f64) -> bool {
        self.fraction(now) < 1.
            && (self.from.position.distance_squared(self.to.position) > 0.000001
                || turn(self.from.facing, self.to.facing).abs() > 0.01)
    }
}

#[derive(Debug, Default)]
pub struct Motion {
    boot: String,
    report: Option<(u64, u64)>,
    tracks: HashMap<String, Track>,
}
impl Motion {
    /// Returns true when the report changed; new objects, a restart or >10m jumps snap.
    pub(crate) fn observe(&mut self, live: &AreaLive, boot: &str, now: f64) -> bool {
        if self.boot != boot {
            self.tracks.clear();
            self.report = None;
            self.boot = boot.to_owned();
        }
        let key = (live.seq, live.received_ms);
        if self.report == Some(key) {
            return false;
        }
        let duration =
            self.report.map_or(0., |(_, t)| live.received_ms.saturating_sub(t) as f64 / 1000.);
        self.report = Some(key);
        self.tracks.retain(|id, _| live.find(id).is_some());
        for (_, o) in live.all() {
            let to = Pose::from(o);
            let track = self.tracks.entry(o.id.clone()).or_insert_with(|| Track {
                from: to,
                to,
                at: now,
                duration: 0.,
                trail: VecDeque::from([(now, to.position)]),
            });
            track.trail.retain(|(t, _)| now - t <= 5.);
            if o.dead || track.to.position.distance(to.position) > 10. {
                track.from = to;
                track.to = to;
                track.duration = 0.;
                track.trail.clear();
            } else if track.to.position != to.position
                || turn(track.to.facing, to.facing).abs() > 0.01
            {
                track.from = track.pose(now);
                track.to = to;
                track.at = now;
                track.duration = duration;
                track.trail.push_back((now, track.from.position));
                while track.trail.len() > 128 {
                    track.trail.pop_front();
                }
            }
        }
        true
    }

    pub(crate) fn apply(&self, o: &mut LiveObject, now: f64) {
        if let Some(t) = self.tracks.get(&o.id) {
            let pose = t.pose(now);
            [o.x, o.y, o.z] = pose.position.to_array();
            o.facing = pose.facing;
        }
    }

    pub(crate) fn position(&self, id: &str, now: f64) -> Option<Vec3> {
        self.tracks.get(id).map(|t| t.pose(now).position)
    }
    pub(crate) fn moving(&self, now: f64) -> bool {
        self.tracks.values().any(|t| t.moving(now))
    }
    pub(crate) fn speed(&self, id: &str, now: f64) -> f32 {
        self.tracks
            .get(id)
            .filter(|t| t.moving(now))
            .map_or(0., |t| t.from.position.distance(t.to.position) / t.duration.max(0.001) as f32)
    }
    pub(crate) fn trail(&self, id: &str, now: f64) -> Vec<Vec3> {
        let Some(t) = self.tracks.get(id) else { return Vec::new() };
        let mut points: Vec<_> =
            t.trail.iter().filter(|(at, _)| now - at <= 5.).map(|(_, p)| *p).collect();
        points.push(t.pose(now).position);
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(seq: u64, ms: u64, x: f32, facing: f32) -> AreaLive {
        AreaLive {
            seq,
            received_ms: ms,
            creatures: vec![LiveObject { id: "1".into(), x, facing, ..Default::default() }],
            ..Default::default()
        }
    }
    #[test]
    fn motion_interpolates_shortest_turn_and_snaps_teleports_restart_and_death() {
        let mut motion = Motion::default();
        let first = report(1, 1000, 0., 350.);
        motion.observe(&first, "a", 0.);
        let next = report(2, 2000, 4., 10.);
        motion.observe(&next, "a", 1.);
        let mut o = next.creatures[0].clone();
        motion.apply(&mut o, 1.5);
        assert_eq!(o.x, 2.);
        assert!(o.facing.abs() < 0.001);
        assert_eq!(motion.speed("1", 1.5), 4.);
        assert!(motion.moving(1.5));
        assert!(!motion.moving(2.));
        assert!(!motion.observe(&next, "a", 2.5));
        motion.observe(&report(3, 3000, 20., 10.), "a", 3.);
        assert_eq!(motion.position("1", 3.), Some(Vec3::new(20., 0., 0.)));
        assert_eq!(motion.trail("1", 3.).len(), 1);
        motion.observe(&next, "b", 4.);
        assert_eq!(motion.position("1", 4.), Some(Vec3::new(4., 0., 0.)));
        let mut dead = report(3, 4000, 6., 90.);
        dead.creatures[0].dead = true;
        motion.observe(&dead, "b", 5.);
        assert_eq!(motion.position("1", 5.), Some(Vec3::new(6., 0., 0.)));
        assert!(!motion.moving(5.));
    }
    #[test]
    fn jitter_is_continuous_trails_expire_and_removed_ids_do_not_reuse_motion() {
        let mut m = Motion::default();
        m.observe(&report(1, 1000, 0., 0.), "a", 0.);
        m.observe(&report(2, 2000, 4., 0.), "a", 1.);
        let at = m.position("1", 1.5);
        m.observe(&report(3, 2500, 6., 0.), "a", 1.5);
        assert_eq!(m.position("1", 1.5), at);
        assert_eq!(m.position("1", 2.), Some(Vec3::new(6., 0., 0.)));
        assert_eq!(m.trail("1", 7.).len(), 1);
        m.observe(&AreaLive { seq: 4, received_ms: 3500, ..Default::default() }, "a", 3.);
        assert!(m.position("1", 3.).is_none());
        m.observe(&report(5, 3500, 50., 0.), "a", 3.);
        assert_eq!(m.position("1", 3.), Some(Vec3::new(50., 0., 0.)));
    }
}
