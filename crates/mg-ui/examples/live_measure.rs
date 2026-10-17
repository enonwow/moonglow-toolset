//! Native, read-only replay profiling. Built only by tools/live/check.ps1 -MeasureGui.
//! No live service, Redis connection, command sender, settings load or module save.
#[path = "live_measure/control.rs"]
mod control;
#[path = "live_measure/distribution.rs"]
mod distribution;
#[cfg(feature = "measure-alloc")]
#[path = "live_measure/heap.rs"]
mod heap;
#[path = "live_measure/os.rs"]
mod os;
#[path = "live_measure/stress.rs"]
mod stress;
#[cfg(feature = "measure-alloc")]
#[global_allocator]
static ALLOCATOR: heap::Counted = heap::Counted;

use mg_ui::{Action, Moonglow, NoDialogs, Tab, live_effects::EffectsMode};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Samples {
    frames: usize,
    discarded_input_events: usize,
    ui_ms: Vec<f64>,
    spans: BTreeMap<&'static str, Vec<f64>>,
    span_calls: BTreeMap<&'static str, u64>,
    causes: BTreeMap<String, u64>,
    pictures: u64,
    report_reads: u64,
    reconciliations: u64,
    minimized_observed: bool,
    startup: Value,
    census: Value,
    gpu: String,
    transient_max: usize,
    speech_max: usize,
    snapshots: BTreeMap<String, Value>,
}

fn memory() -> Value {
    let mut value = os::memory();
    #[cfg(feature = "measure-alloc")]
    {
        value["rust_heap"] = heap::snapshot();
    }
    value["note"] = "Virtual committed regions overlap process-private accounting; not physical RAM or exclusive cache ownership.".into();
    value
}
fn stats(mut v: Vec<f64>) -> Value {
    if v.is_empty() {
        return json!({"n":0});
    }
    v.sort_by(f64::total_cmp);
    json!({"n":v.len(),"mean_ms":v.iter().sum::<f64>()/v.len() as f64,"p50_ms":v[v.len()/2],"p95_ms":v[(v.len()-1)*95/100],"max_ms":v[v.len()-1]})
}

struct Replay {
    app: Moonglow,
    area: mg_core::ResRef,
    case: String,
    samples: Arc<Mutex<Samples>>,
    collecting: Arc<AtomicBool>,
    ready: Option<std::sync::mpsc::Sender<()>>,
    stage: u8,
    additional_areas: Vec<mg_core::ResRef>,
    start: Instant,
    last_pictures: u64,
    last_reads: u64,
    last_reconciliations: u64,
}
impl eframe::App for Replay {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        // Measure an idle replay, even while the owner uses another window.
        // Preserve OS viewport/minimization data, but never accept live input.
        if self.collecting.load(Ordering::Relaxed) && !input.events.is_empty() {
            self.samples.lock().unwrap().discarded_input_events += input.events.len();
        }
        input.events.clear();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.stage == 0
            && let Some(v) = self.app.area_views.get_mut(&self.area)
        {
            v.live.on = true;
            if self.case == "static" || self.case == "reports-only" {
                // Isolated static control: keep the report, hide objects and tile animation.
                // This changes only this replay's in-memory drawing, never the workspace/module.
                v.show = [false; 9];
                if let Some(model) = &mut v.model {
                    model.tiles.clear();
                }
            }
            v.live.effects =
                if self.case.contains("full") { EffectsMode::Full } else { EffectsMode::Markers };
            if let Some(o) = &mut v.orbit {
                o.target = glam::vec3(25., 25., 0.);
                o.distance = 44.;
            }
            for area in &self.additional_areas {
                if let Some(v) = self.app.area_views.get_mut(area) {
                    v.live.on = true;
                }
            }
            self.stage = 1;
        }
        let t = Instant::now();
        self.app.ui(ui);
        let elapsed = t.elapsed().as_secs_f64() * 1000.;
        let spans = mg_ui::measure::take();
        if self.stage == 1
            && self.app.area_views.get(&self.area).is_some_and(|v| v.live.report_reads > 0)
        {
            self.samples.lock().unwrap().startup["first_live_frame_ms"] =
                self.start.elapsed().as_secs_f64().mul_add(1000., 0.).into();
            self.samples.lock().unwrap().snapshots.insert("first_live_frame".into(), memory());
            if self.case == "macros" {
                let dir = self.app.live_scene.data_dir.as_ref().unwrap();
                let scene = mg_live::scene::load(dir, "DEMO-Live-01", "Measured macro 00").unwrap();
                self.app.live_scene.draft.steps = scene.steps.into_iter().map(Into::into).collect();
            }
            if self.case == "areas-loaded" {
                for area in &self.additional_areas {
                    let old = self.app.dock.find_tab(&Tab::Area(*area)).unwrap();
                    self.app.dock.remove_tab(old);
                    let first = self.app.dock.find_tab(&Tab::Area(self.area)).unwrap();
                    self.app.dock.main_surface_mut().split_below(
                        first.node,
                        0.5,
                        vec![Tab::Area(*area)],
                    );
                }
            }
            let tab = if matches!(self.case.as_str(), "focus-loaded" | "areas-loaded") {
                Tab::LiveFocus
            } else if self.case == "legend" {
                Tab::LiveLegend
            } else if self.case == "macros" {
                Tab::Scene
            } else if self.case == "panel" {
                Tab::LivePanel
            } else if self.case == "log" || self.case == "log-loaded" {
                Tab::LiveLog
            } else {
                Tab::LiveChat
            };
            self.app.actions.push(Action::OpenTab(tab));
            self.stage = 2;
        } else if self.stage == 2 {
            self.samples.lock().unwrap().census = mg_ui::measure::census(&self.app, &self.area);
            self.samples.lock().unwrap().census["focus_rows"] =
                json!(self.app.live_focus.entries.iter().map(|e| e.rows.len()).sum::<usize>());
            self.samples.lock().unwrap().census["focus_entries"] =
                json!(self.app.live_focus.entries.len());
            self.samples.lock().unwrap().census["live_maps"] = json!(self.app.area_views.iter().map(|(a,v)|json!({"area":a.to_string(),"on":v.live.on,"reads":v.live.report_reads})).collect::<Vec<_>>());
            self.samples.lock().unwrap().census["area_views"] = json!(self.app.area_views.len());
            self.samples.lock().unwrap().census["macro_steps"] =
                json!(self.app.live_scene.draft.steps.len());
            self.samples.lock().unwrap().census["viewport"] = json!({
                "inner_points":ui.input(|i|i.viewport().inner_rect.map(|r|[r.width(),r.height()])),
                "pixels_per_point":ui.ctx().pixels_per_point(),
            });
            if self.case == "hidden" {
                let tab = self.app.dock.find_tab(&Tab::Area(self.area)).unwrap();
                self.app.dock.set_focused_node_and_surface(egui_dock::NodePath {
                    surface: tab.surface,
                    node: tab.node,
                });
                self.app.dock.push_to_focused_leaf(Tab::ModuleProperties);
                let front = self.app.dock.find_tab(&Tab::ModuleProperties).unwrap();
                let _ = self.app.dock.set_active_tab(front);
            }
            if self.case == "minimized" {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            if let Some(tx) = self.ready.take() {
                tx.send(()).unwrap();
            }
            self.stage = 3;
        }
        if let Some(v) = self.app.area_views.get(&self.area) {
            if self.collecting.load(Ordering::Relaxed) {
                let mut out = self.samples.lock().unwrap();
                out.frames += 1;
                out.transient_max = out.transient_max.max(v.live.transient_effects);
                out.speech_max = out.speech_max.max(v.live.speech_bubbles);
                out.ui_ms.push(elapsed);
                out.pictures += v.pictures.saturating_sub(self.last_pictures);
                out.report_reads += v.live.report_reads.saturating_sub(self.last_reads);
                out.reconciliations +=
                    v.live.reconciliations.saturating_sub(self.last_reconciliations);
                out.minimized_observed |= ui.input(|i| i.viewport().minimized.unwrap_or(false));
                for (name, (calls, ms)) in spans {
                    out.spans.entry(name).or_default().push(ms);
                    *out.span_calls.entry(name).or_default() += calls;
                }
                for cause in ui.ctx().repaint_causes() {
                    *out.causes.entry(cause.to_string()).or_default() += 1;
                }
            }
            self.last_pictures = v.pictures;
            self.last_reads = v.live.report_reads;
            self.last_reconciliations = v.live.reconciliations;
        }
        if self.stage < 3 {
            ui.ctx().request_repaint();
        }
    }
}

fn main() -> eframe::Result<()> {
    if let Some(root) = std::env::var_os("MGLIVE_STRESS_CONTROL") {
        control::run(&PathBuf::from(root));
        return Ok(());
    }
    mg_testkit::gpu::hold();
    if std::env::var_os("MGLIVE_STRESS_REPLAY").is_some() {
        return stress::run();
    }
    let start = Instant::now();
    let out = PathBuf::from(
        std::env::var("MGLIVE_MEASURE_OUT").expect("explicit scratch output required"),
    );
    let case = std::env::var("MGLIVE_MEASURE_CASE").unwrap_or("moving".into());
    let seconds = std::env::var("MGLIVE_MEASURE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(12);
    let profile = std::env::var("MGLIVE_MEASURE_PROFILE").unwrap_or("unspecified".into());
    let root = PathBuf::from(std::env::var("NWN_ROOT").expect("game installation required"));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture_path = repo.join("target/test-output/live-next/review-profile-fixture.json");
    let mut fixture: Value =
        serde_json::from_slice(&std::fs::read(&fixture_path).unwrap()).unwrap();
    let area: mg_core::ResRef = fixture["areas"][0]["area"].as_str().unwrap().parse().unwrap();
    if case.contains("effects") {
        for creature in fixture["areas"][0]["cre"].as_array_mut().unwrap().iter_mut().take(16) {
            creature["vfx"] = json!([7]);
        }
    }
    let additional_areas: Vec<mg_core::ResRef> = if case == "areas-loaded" {
        vec!["measuretwo".parse().unwrap(), "measurethree".parse().unwrap()]
    } else {
        Vec::new()
    };
    for (i, area) in additional_areas.iter().enumerate() {
        let mut copy = fixture["areas"][0].clone();
        copy["area"] = area.to_string().into();
        copy["cre"].as_array_mut().unwrap().truncate(8);
        for (n, o) in copy["cre"].as_array_mut().unwrap().iter_mut().enumerate() {
            o["id"] = format!("{:x}", 0x2000 + i * 0x100 + n).into();
        }
        for key in ["plc", "doors", "items"] {
            copy[key] = json!([]);
        }
        fixture["areas"].as_array_mut().unwrap().push(copy);
    }
    if case.ends_with("-loaded") {
        fixture["events"] = Value::Array(
            (1..=1000)
                .map(|n| {
                    let a = &fixture["areas"][n % fixture["areas"].as_array().unwrap().len()];
                    json!({
                        "seq":n,"kind":"chat","area":a["area"],"id":a["cre"][0]["id"],
                        "name":"Measured NPC","text":format!("Replay chat message {n}"),"channel":0
                    })
                })
                .collect(),
        );
    }
    let store = Arc::new(mg_live::store::LiveStore::default());
    store.sync(&serde_json::to_vec(&fixture).unwrap(), mg_live::now_ms()).unwrap();
    fixture["events"] = json!([]);
    let samples = Arc::new(Mutex::new(Samples::default()));
    samples.lock().unwrap().snapshots.insert("before_gpu".into(), memory());
    let collecting = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600., 1000.])
            .with_title(format!("Live measurement: {profile} / {case}"))
            .with_active(false),
        persist_window: false,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(egui_wgpu::WgpuSetupCreateNew {
                device_descriptor: Arc::new(|adapter| wgpu::DeviceDescriptor {
                    required_features: adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC,
                    required_limits: adapter.limits(),
                    ..Default::default()
                }),
                ..egui_wgpu::WgpuSetupCreateNew::without_display_handle()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Moonglow Live Measurement",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.disable_accesskit();
            samples.lock().unwrap().gpu =
                format!("{:?}", cc.wgpu_render_state.as_ref().unwrap().adapter.get_info());
            samples.lock().unwrap().snapshots.insert("gpu_ready".into(), memory());
            let t = Instant::now();
            let mut app = Moonglow::new(
                Some(mg_resman::GameInstall::new(
                    &root,
                    Some(repo.join("target/test-output/live-demo/user")),
                    "en",
                )),
                Box::new(NoDialogs::default()),
            );
            samples.lock().unwrap().snapshots.insert("resources_ready".into(), memory());
            app.set_render_state(cc.wgpu_render_state.clone().unwrap());
            app.open_module(&repo.join("target/test-output/live-demo/DEMO-Live-01.mod"));
            samples.lock().unwrap().snapshots.insert("module_ready".into(), memory());
            samples.lock().unwrap().startup = json!({"gpu_ready_ms":t.duration_since(start).as_secs_f64()*1000.,"app_module_ms":t.elapsed().as_secs_f64()*1000.});
            app.no_plugins = true;
            app.open_palette = false;
            app.live = Some(store.clone());
            app.settings.live.enabled = true;
            app.settings.still_objects = matches!(
                case.as_str(),
                "static" | "reports-only" | "quiet" | "reports" | "cues" | "speech"
            );
            let scratch = repo.join("target/test-output/live-opt-ui/fixture-data");
            app.live_focus.data_dir = Some(scratch.clone());
            app.live_focus.context(app.module_path());
            // A preceding Focus case saved a watch in this same isolated working set.
            // Case order must not add hidden Focus work to unrelated controls.
            app.live_focus.entries.clear();
            app.live_focus.selected = None;
            app.live_scene.data_dir = Some(scratch.clone());
            if matches!(case.as_str(), "focus-loaded" | "areas-loaded") {
                let ids: Vec<String> = fixture["areas"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a["cre"][0]["id"].as_str().unwrap().to_owned())
                    .collect();
                app.live_focus.add(&store, &ids).unwrap();
            }
            if case == "macros" {
                use mg_live::scene::{self, Scene, Step, Wait};
                let steps: Vec<Step> = (0..20).map(|n|Step {delay_ms:1000,op:"live_npc_say".into(),input:json!({"area":area.to_string(),"target":fixture["areas"][0]["cre"][0]["id"],"text":format!("Macro step {n}")}),wait:Wait::Nothing}).collect();
                for n in 0..20 {
                    scene::save(
                        &scratch,
                        "DEMO-Live-01",
                        &Scene { name: format!("Measured macro {n:02}"), steps: steps.clone() },
                    )
                    .unwrap();
                }
                app.live_scene.draft.steps = steps.into_iter().map(Into::into).collect();
            }
            for other in &additional_areas {
                for ty in [mg_core::ResType::ARE, mg_core::ResType::GIT, mg_core::ResType::GIC] {
                    let ws = app.ws.as_mut().unwrap();
                    if let Some(bytes) =
                        ws.module.get(&mg_resman::ResKey::new(area, ty)).map(|b| b.to_vec())
                    {
                        ws.module.set(mg_resman::ResKey::new(*other, ty), bytes);
                    }
                }
                app.actions.push(Action::OpenTab(Tab::Area(*other)));
            }
            app.actions.push(Action::OpenTab(Tab::Area(area)));
            let (ctx, samples2, collecting2, store2, case2) = (
                cc.egui_ctx.clone(),
                samples.clone(),
                collecting.clone(),
                store.clone(),
                case.clone(),
            );
            std::thread::spawn(move || {
                ready_rx.recv_timeout(Duration::from_secs(90)).expect("first Live frame");
                std::thread::sleep(Duration::from_secs(2));
                let mem_start = memory();
                let cpu_start = os::cpu();
                let wall = Instant::now();
                let native_frame_start = ctx.cumulative_frame_nr();
                collecting2.store(true, Ordering::Relaxed);
                let mut reports = 0u64;
                let mut wakes = 0u64;
                let mut producer_ms = 0.;
                let original = fixture.clone();
                let emits = case2 != "quiet" && case2 != "static";
                for n in 0..seconds {
                    if emits {
                        let producer_started = Instant::now();
                        fixture["seq"] = (n + 100).into();
                        if matches!(
                            case2.as_str(),
                            "moving"
                                | "effects-markers"
                                | "effects-full"
                                | "panel"
                                | "log"
                                | "log-loaded"
                                | "chat-loaded"
                                | "focus-loaded"
                                | "areas-loaded"
                                | "macros"
                                | "legend"
                                | "hidden"
                                | "minimized"
                        ) {
                            for (i, o) in fixture["areas"][0]["cre"]
                                .as_array_mut()
                                .unwrap()
                                .iter_mut()
                                .enumerate()
                            {
                                o["x"] = (original["areas"][0]["cre"][i]["x"].as_f64().unwrap()
                                    + ((n as f64) * 0.7).sin() * 1.5)
                                    .into();
                            }
                        }
                        if case2 == "cues" || case2 == "speech" {
                            fixture["events"] = json!([{"seq":n+100,"kind":if case2=="speech" {"chat"}else{"damage"},"area":area.to_string(),"id":"1000","target":"1001","text":"Measured replay speech","amount":7}]);
                        }
                        store2
                            .sync(&serde_json::to_vec(&fixture).unwrap(), mg_live::now_ms())
                            .unwrap();
                        producer_ms += producer_started.elapsed().as_secs_f64() * 1000.;
                        reports += 1;
                        // Same wake semantics as the bridge's accepted report callback; no Redis writes.
                        mg_ui::live_repaint::wake(&ctx);
                        wakes += 1;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
                collecting2.store(false, Ordering::Relaxed);
                let elapsed = wall.elapsed().as_secs_f64();
                let cpu = os::cpu() - cpu_start;
                let native_frames = ctx.cumulative_frame_nr().saturating_sub(native_frame_start);
                let mem_end = memory();
                let status = store2.status(mg_live::now_ms());
                assert!(status["queued"].as_array().unwrap().is_empty(), "replay queued a command");
                assert!(
                    status["results"].as_array().unwrap().is_empty(),
                    "replay has command results"
                );
                let mut s = samples2.lock().unwrap();
                let spans: BTreeMap<_, _> =
                    std::mem::take(&mut s.spans).into_iter().map(|(k, v)| (k, stats(v))).collect();
                let result = json!({
                    "profile":profile,
                    "case":case2,
                    "pid":std::process::id(),
                    "duration_s":elapsed,
                    "cpu_s":cpu,
                    "cpu_one_core_percent":100.*cpu/elapsed,
                    "frames":s.frames,
                    "discarded_input_events":s.discarded_input_events,
                    "fps":s.frames as f64/elapsed,
                    "native_frames":native_frames,
                    "native_frame_hz":native_frames as f64/elapsed,
                    "area_pictures":s.pictures,
                    "area_picture_hz":s.pictures as f64/elapsed,
                    "ui":stats(std::mem::take(&mut s.ui_ms)),
                    "spans":spans,
                    "span_calls":s.span_calls,
                    "egui_repaint_causes":s.causes,
                    "synthetic_report_wakes":wakes,
                    "reports":reports,
                    "producer_total_ms":producer_ms,
                    "report_reads":s.report_reads,
                    "reconciliations":s.reconciliations,
                    "startup":s.startup,
                    "memory_start":mem_start,
                    "memory_end":mem_end,
                    "memory_stages":s.snapshots,
                    "minimized_observed":s.minimized_observed,
                    "minimized_os":os::minimized(),
                    "gpu":s.gpu,
                    "census":s.census,
                    "transient_max":s.transient_max,
                    "speech_max":s.speech_max,
                    "requested_screen_points":[1600,1000],
                    "fixture":fixture_path,
                    "objects":original["areas"].as_array().unwrap().iter().map(|a|
                        ["cre", "plc", "doors", "items"].iter().map(|k|
                            a[k].as_array().map_or(0, Vec::len)).sum::<usize>()
                    ).sum::<usize>(),
                    "crowd":218,
                    "queued_commands":0,
                    "heap_accounting":cfg!(feature="measure-alloc"),
                    "method":"native eframe/winit event loop, present pacing; fake store, original fixture, no game commands; process CPU includes all threads and final egui painting"
                });
                std::fs::write(&out, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                drop(s);
                // A minimized OS window may not run another UI pass; end only this owned helper.
                std::thread::sleep(Duration::from_secs(3));
                std::process::exit(0);
            });
            Ok(Box::new(Replay {
                app,
                area,
                case,
                samples,
                collecting,
                ready: Some(ready_tx),
                stage: 0,
                additional_areas,
                start,
                last_pictures: 0,
                last_reads: 0,
                last_reconciliations: 0,
            }))
        }),
    )
}
