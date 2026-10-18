//! Scenes: a named list of live operations played in order, each after a
//! delay, optionally waiting for its result or for an object to come near a
//! point. Saved per module as `<data_dir>/live/scenes/<module>/<name>.json`.
//!
//! A step is an operation of the agent table (`live_npc_move`, `live_npc_say`,
//! ...) with the input an agent would send, so the policy, the schemas and
//! the protection of players apply to a scene as to any agent.

use crate::{operations, store::LiveStore};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

pub const MAX_STEPS: usize = 200;
pub const MAX_DELAY_MS: u64 = 600_000;
pub const MAX_TIMEOUT_MS: u64 = 120_000;
/// How often a wait looks again.
const POLL_MS: u64 = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub name: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Milliseconds after the previous step (and its wait) finished.
    #[serde(default)]
    pub delay_ms: u64,
    /// An operation of the agent table.
    pub op: String,
    #[serde(default = "empty_object")]
    pub input: Value,
    #[serde(default)]
    pub wait: Wait,
}

fn empty_object() -> Value {
    json!({})
}

/// What a step waits for before the next one's delay starts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "until", rename_all = "snake_case", deny_unknown_fields)]
pub enum Wait {
    #[default]
    Nothing,
    /// The game's result of the command the step queued.
    Result { timeout_ms: u64 },
    /// An object within `radius` metres of (`x`, `y`) in `area`.
    Near { area: String, target: String, x: f32, y: f32, radius: f32, timeout_ms: u64 },
}

/// A scene's or a module's name as a file name: letters, digits, space,
/// `_`, `-` and `.`, not starting with `.` or a space.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with(['.', ' '])
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b" _-.".contains(&b))
}

impl Scene {
    /// Its shape, before it is saved or played.
    pub fn check(&self) -> Result<(), String> {
        if !valid_name(&self.name) {
            return Err("A scene name is 1-64 letters, digits, spaces, _ - or .".into());
        }
        if self.steps.is_empty() || self.steps.len() > MAX_STEPS {
            return Err(format!("A scene has 1-{MAX_STEPS} steps"));
        }
        for (i, step) in self.steps.iter().enumerate() {
            let n = i + 1;
            // (A scene does not wait or play scenes: its own waits do that.)
            if !step.op.starts_with("live_")
                || step.op == "live_wait"
                || step.op.starts_with("live_scene")
            {
                return Err(format!("Step {n}: {} cannot be a step", step.op));
            }
            if !step.input.is_object() {
                return Err(format!("Step {n}: input must be an object"));
            }
            if step.delay_ms > MAX_DELAY_MS {
                return Err(format!("Step {n}: delay over {MAX_DELAY_MS} ms"));
            }
            let timeout = match &step.wait {
                Wait::Nothing => 0,
                Wait::Result { timeout_ms } => *timeout_ms,
                Wait::Near { radius, timeout_ms, .. } => {
                    if !(radius.is_finite() && *radius > 0.0) {
                        return Err(format!("Step {n}: radius must be above 0"));
                    }
                    *timeout_ms
                }
            };
            if timeout > MAX_TIMEOUT_MS {
                return Err(format!("Step {n}: wait over {MAX_TIMEOUT_MS} ms"));
            }
        }
        Ok(())
    }
}

/// The time a scene plays by (a fake one in tests).
pub trait Clock {
    fn now_ms(&self) -> u64;
    fn sleep(&self, ms: u64);
}

#[derive(Debug)]
pub struct RealClock;

impl Clock for RealClock {
    fn now_ms(&self) -> u64 {
        crate::now_ms()
    }
    fn sleep(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// `ms` waited, a slice at a time, unless `stop` is set.
fn pause(clock: &dyn Clock, stop: &AtomicBool, ms: u64) -> Result<(), String> {
    let end = clock.now_ms().saturating_add(ms);
    loop {
        if stop.load(Ordering::Acquire) {
            return Err("Scene stopped".into());
        }
        let now = clock.now_ms();
        if now >= end {
            return Ok(());
        }
        clock.sleep((end - now).min(POLL_MS));
    }
}

/// Looks with `found` until it gives something, for `timeout_ms`.
fn until(
    clock: &dyn Clock,
    stop: &AtomicBool,
    timeout_ms: u64,
    mut found: impl FnMut() -> Result<Option<Value>, String>,
) -> Result<Value, String> {
    let end = clock.now_ms().saturating_add(timeout_ms);
    loop {
        if let Some(value) = found()? {
            return Ok(value);
        }
        if clock.now_ms() >= end {
            return Err(format!("no result within {timeout_ms} ms"));
        }
        pause(clock, stop, POLL_MS)?;
    }
}

/// Plays `scene` to its end, the first step that fails, or `stop`. Each
/// step's outcome: its number, when it ran (ms from the start) and what
/// the operation (and its wait) gave.
pub fn play(
    scene: &Scene,
    live: &LiveStore,
    stop: &AtomicBool,
    clock: &dyn Clock,
) -> Result<Vec<Value>, String> {
    scene.check()?;
    // Every operation offered now, before anything is sent.
    let offered = operations::describe(live.policy());
    for (i, step) in scene.steps.iter().enumerate() {
        if !offered["operations"].as_array().into_iter().flatten().any(|o| o["name"] == step.op) {
            return Err(format!("Step {}: {} is unknown or disabled", i + 1, step.op));
        }
    }
    let start = clock.now_ms();
    let mut out = Vec::new();
    for (i, step) in scene.steps.iter().enumerate() {
        let n = i + 1;
        pause(clock, stop, step.delay_ms)?;
        let at_ms = clock.now_ms() - start;
        let mut result = operations::call_scoped(
            &step.op,
            step.input.clone(),
            live,
            stop,
            &crate::access::Scope::local(),
        )
        .map_err(|e| format!("Step {n}: {e}"))?;
        match &step.wait {
            Wait::Nothing => {}
            Wait::Result { timeout_ms } => {
                let id = result["queued"].as_str().unwrap_or_default().to_string();
                if id.is_empty() {
                    return Err(format!("Step {n}: {} queues no command to wait for", step.op));
                }
                result = until(clock, stop, *timeout_ms, || {
                    let status = live.status(crate::now_ms());
                    Ok(status["results"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|r| r["result"]["id"] == id.as_str())
                        .map(|r| r["result"].clone()))
                })
                .map_err(|e| format!("Step {n}: {e}"))?;
                if result["ok"] == false {
                    let error = result["err"].as_str().unwrap_or("failed");
                    return Err(format!("Step {n}: the game says: {error}"));
                }
            }
            Wait::Near { area, target, x, y, radius, timeout_ms } => {
                until(clock, stop, *timeout_ms, || {
                    let state = live
                        .area_json(area, crate::now_ms())
                        .ok_or_else(|| format!("no report for area {area}"))?;
                    let near = ["creatures", "placeables", "doors", "items"]
                        .iter()
                        .flat_map(|k| state[*k].as_array().into_iter().flatten())
                        .find(|o| o["id"] == target.as_str())
                        .and_then(|o| Some((o["x"].as_f64()?, o["y"].as_f64()?)))
                        .is_some_and(|(ox, oy)| {
                            (ox - f64::from(*x)).hypot(oy - f64::from(*y)) <= f64::from(*radius)
                        });
                    Ok(near.then(|| json!(true)))
                })
                .map_err(|e| format!("Step {n}: {target} {e}"))?;
            }
        }
        out.push(json!({"step": n, "at_ms": at_ms, "result": result}));
    }
    Ok(out)
}

type Playing = (String, Arc<AtomicBool>, JoinHandle<Result<Vec<Value>, String>>);

/// One scene playing at a time, on its own thread.
#[derive(Debug, Default)]
pub struct Player {
    playing: Option<Playing>,
    last: Option<(String, Result<Vec<Value>, String>)>,
}

impl Player {
    pub fn start(&mut self, scene: Scene, live: Arc<LiveStore>) -> Result<(), String> {
        self.status();
        if let Some((name, ..)) = &self.playing {
            return Err(format!("Scene {name} is playing; stop it first"));
        }
        scene.check()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let name = scene.name.clone();
        let thread = std::thread::spawn(move || play(&scene, &live, &flag, &RealClock));
        self.playing = Some((name, stop, thread));
        Ok(())
    }

    /// Stops the scene playing, if one is, and waits for it to end.
    pub fn stop(&mut self) {
        if let Some((name, stop, thread)) = self.playing.take() {
            stop.store(true, Ordering::Release);
            let result = thread.join().unwrap_or_else(|_| Err("Scene thread failed".into()));
            self.last = Some((name, result));
        }
    }

    /// `{"playing": name or null, "last": {"name", "ok", "steps" | "error"}}`.
    pub fn status(&mut self) -> Value {
        if self.playing.as_ref().is_some_and(|(_, _, t)| t.is_finished()) {
            let (name, _, thread) = self.playing.take().expect("checked above");
            let result = thread.join().unwrap_or_else(|_| Err("Scene thread failed".into()));
            self.last = Some((name, result));
        }
        let last = self.last.as_ref().map(|(name, result)| match result {
            Ok(steps) => json!({"name": name, "ok": true, "steps": steps}),
            Err(error) => json!({"name": name, "ok": false, "error": error}),
        });
        json!({"playing": self.playing.as_ref().map(|(n, ..)| n), "last": last})
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The folder of a module's scenes.
pub fn folder(data_dir: &Path, module: &str) -> Result<PathBuf, String> {
    if !valid_name(module) {
        return Err("Invalid module name".into());
    }
    Ok(data_dir.join("live").join("scenes").join(module))
}

pub fn save(data_dir: &Path, module: &str, scene: &Scene) -> Result<(), String> {
    scene.check()?;
    let dir = folder(data_dir, module)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.json", scene.name));
    let text = serde_json::to_string_pretty(scene).map_err(|e| e.to_string())?;
    // (Written aside and renamed: a scene is never left half written.)
    let part = path.with_extension("json.part");
    std::fs::write(&part, text).map_err(|e| format!("{}: {e}", part.display()))?;
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load(data_dir: &Path, module: &str, name: &str) -> Result<Scene, String> {
    if !valid_name(name) {
        return Err("Invalid scene name".into());
    }
    let path = folder(data_dir, module)?.join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let scene: Scene =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    scene.check()?;
    Ok(scene)
}

/// The module's scenes' names, sorted.
pub fn list(data_dir: &Path, module: &str) -> Result<Vec<String>, String> {
    let dir = folder(data_dir, module)?;
    let Ok(entries) = std::fs::read_dir(&dir) else { return Ok(Vec::new()) };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json").map(str::to_string))
        .filter(|n| valid_name(n))
        .collect();
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// Time that moves only when slept; `on_sleep` runs at each sleep.
    struct Fake {
        now: Cell<u64>,
        on_sleep: RefCell<Box<dyn FnMut(u64)>>,
    }

    impl Fake {
        fn new(on_sleep: impl FnMut(u64) + 'static) -> Fake {
            Fake { now: Cell::new(1_000), on_sleep: RefCell::new(Box::new(on_sleep)) }
        }
    }

    impl Clock for Fake {
        fn now_ms(&self) -> u64 {
            self.now.get()
        }
        fn sleep(&self, ms: u64) {
            self.now.set(self.now.get() + ms);
            (self.on_sleep.borrow_mut())(self.now.get());
        }
    }

    fn room(live: &LiveStore, seq: u64, extra: Value) {
        let mut report = json!({"v":1,"boot":"scene","seq":seq,"areas":[{"area":"room","full":true,
            "cre":[{"id":"7","tag":"GUARD","r":"guard","name":"Guard","x":1.0,"y":1.0,"z":0.0,"f":0.0}]}]});
        for (k, v) in extra.as_object().unwrap() {
            report[k] = v.clone();
        }
        live.sync(report.to_string().as_bytes(), crate::now_ms()).unwrap();
    }

    fn step(delay_ms: u64, op: &str, input: Value, wait: Wait) -> Step {
        Step { delay_ms, op: op.into(), input, wait }
    }

    #[test]
    fn steps_play_after_their_delays_and_wait_for_results() {
        let live = Arc::new(LiveStore::default());
        room(&live, 1, json!({}));
        let scene = Scene {
            name: "greeting".into(),
            steps: vec![
                step(
                    100,
                    "live_npc_say",
                    json!({"area":"room","target":"7","text":"Halt!"}),
                    Wait::Nothing,
                ),
                step(
                    250,
                    "live_run_script",
                    json!({"area":"room","script":"example","id":"scene-run"}),
                    Wait::Result { timeout_ms: 1_000 },
                ),
                step(
                    0,
                    "live_npc_say",
                    json!({"area":"room","target":"7","text":"Go."}),
                    Wait::Nothing,
                ),
            ],
        };
        // The game answers the script once 300 ms have gone.
        let store = live.clone();
        let mut answered = false;
        let clock = Fake::new(move |now| {
            if now >= 1_000 + 400 && !answered {
                answered = true;
                room(
                    &store,
                    2,
                    json!({"ack":["scene-run"],"results":[{"id":"scene-run","ok":true}]}),
                );
            }
        });
        let out = play(&scene, &live, &AtomicBool::new(false), &clock).unwrap();
        let at: Vec<u64> = out.iter().map(|o| o["at_ms"].as_u64().unwrap()).collect();
        assert_eq!(at[..2], [100, 350]);
        assert!(at[2] >= 400 && at[2] <= 400 + POLL_MS, "after the result: {at:?}");
        assert_eq!(out[1]["result"]["ok"], true);
        assert!(out[0]["result"]["queued"].is_string());
    }

    #[test]
    fn local_scene_can_run_while_agents_are_disabled() {
        let live = LiveStore::default();
        room(&live, 1, json!({}));
        live.set_agent_consent(false);
        let scene = Scene {
            name: "local".into(),
            steps: vec![step(
                0,
                "live_npc_say",
                json!({"area":"room","target":"7","text":"Hello"}),
                Wait::Nothing,
            )],
        };
        let out = play(&scene, &live, &AtomicBool::new(false), &Fake::new(|_| {})).unwrap();
        assert!(out[0]["result"]["queued"].is_string());
        assert!(live.commands(crate::now_ms())["commands"][0].get("by").is_none());
    }

    #[test]
    fn a_failed_command_stops_the_scene_with_the_game_s_reason() {
        let live = Arc::new(LiveStore::default());
        room(&live, 1, json!({}));
        let scene = Scene {
            name: "f".into(),
            steps: vec![step(
                0,
                "live_run_script",
                json!({"area":"room","script":"example","id":"scene-fail"}),
                Wait::Result { timeout_ms: 1_000 },
            )],
        };
        let store = live.clone();
        let mut answered = false;
        let clock = Fake::new(move |_| {
            if !answered {
                answered = true;
                let result =
                    json!({"id":"scene-fail","ok":false,"err":"target is dead","code":"dead"});
                room(&store, 2, json!({"ack":["scene-fail"],"results":[result]}));
            }
        });
        let err = play(&scene, &live, &AtomicBool::new(false), &clock).unwrap_err();
        assert_eq!(err, "Step 1: the game says: target is dead");
    }

    #[test]
    fn waits_time_out_fail_and_stop() {
        let live = LiveStore::default();
        room(&live, 1, json!({}));
        let wait = |wait| Scene {
            name: "w".into(),
            steps: vec![step(
                0,
                "live_run_script",
                json!({"area":"room","script":"example"}),
                wait,
            )],
        };
        let clock = Fake::new(|_| {});
        let err =
            play(&wait(Wait::Result { timeout_ms: 500 }), &live, &AtomicBool::new(false), &clock)
                .unwrap_err();
        assert!(err.contains("Step 1: no result within 500 ms"), "{err}");

        // The guard stands at (1, 1): near (2, 1) within 1.5 m, not within 0.5 m.
        let near = |radius| Wait::Near {
            area: "room".into(),
            target: "7".into(),
            x: 2.0,
            y: 1.0,
            radius,
            timeout_ms: 200,
        };
        assert!(play(&wait(near(1.5)), &live, &AtomicBool::new(false), &clock).is_ok());
        let err = play(&wait(near(0.5)), &live, &AtomicBool::new(false), &clock).unwrap_err();
        assert!(err.contains("7 no result within 200 ms"), "{err}");

        let stop = AtomicBool::new(true);
        assert_eq!(play(&wait(Wait::Nothing), &live, &stop, &clock).unwrap_err(), "Scene stopped");
    }

    #[test]
    fn bad_scenes_are_refused_before_anything_is_sent() {
        let live = LiveStore::default();
        room(&live, 1, json!({}));
        let clock = Fake::new(|_| {});
        let one = |op: &str, input| Scene {
            name: "s".into(),
            steps: vec![step(0, op, input, Wait::Nothing)],
        };
        for (op, why) in [
            ("live_wait", "cannot be a step"),
            ("live_scene_run", "cannot be a step"),
            ("rm", "cannot be a step"),
        ] {
            assert!(one(op, json!({})).check().unwrap_err().contains(why));
        }
        // Chunks are off by default: the whole scene is refused, nothing queued.
        let scene = Scene {
            name: "s".into(),
            steps: vec![
                step(
                    0,
                    "live_npc_say",
                    json!({"area":"room","target":"7","text":"a"}),
                    Wait::Nothing,
                ),
                step(0, "live_run_chunk", json!({"area":"room","code":"x"}), Wait::Nothing),
            ],
        };
        let err = play(&scene, &live, &AtomicBool::new(false), &clock).unwrap_err();
        assert!(err.contains("Step 2: live_run_chunk is unknown or disabled"), "{err}");
        assert!(live.status(crate::now_ms())["queued"].as_array().is_none_or(|q| q.is_empty()));
        assert!(Scene { name: "../x".into(), steps: scene.steps.clone() }.check().is_err());
        assert!(Scene { name: "s".into(), steps: vec![] }.check().is_err());
    }

    #[test]
    fn scenes_are_saved_per_module_and_played_on_a_thread() {
        let dir = std::env::temp_dir().join(format!("mg-live-scenes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let scene = Scene {
            name: "ambush 1".into(),
            steps: vec![step(
                10,
                "live_npc_say",
                json!({"area":"room","target":"7","text":"Now!"}),
                Wait::Near {
                    area: "room".into(),
                    target: "7".into(),
                    x: 1.0,
                    y: 1.0,
                    radius: 2.0,
                    timeout_ms: 100,
                },
            )],
        };
        save(&dir, "DEMO-Live-01", &scene).unwrap();
        assert_eq!(list(&dir, "DEMO-Live-01").unwrap(), ["ambush 1"]);
        assert_eq!(load(&dir, "DEMO-Live-01", "ambush 1").unwrap(), scene);
        assert!(list(&dir, "other").unwrap().is_empty());
        assert!(save(&dir, "..", &scene).is_err() && load(&dir, "DEMO-Live-01", "../x").is_err());

        let live = Arc::new(LiveStore::default());
        room(&live, 1, json!({}));
        let mut player = Player::default();
        player.start(scene.clone(), live.clone()).unwrap();
        assert!(player.start(scene.clone(), live.clone()).unwrap_err().contains("is playing"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while player.status()["playing"].is_string() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(player.status()["last"]["ok"], true, "{}", player.status());
        player.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
