//! Long native replay soak. No Redis/API socket or game command transport exists.
#[path = "replay.rs"]
mod replay;
use super::{distribution::Distribution, os};
use mg_ui::{Action, Moonglow, NoDialogs, Tab, live_effects::EffectsMode};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Bucket {
    timings: BTreeMap<String, Distribution>,
    counts: BTreeMap<String, u64>,
}
impl Bucket {
    fn timing(&mut self, name: &str, ms: f64) {
        self.timings.entry(name.into()).or_default().add(ms);
    }
    fn count(&mut self, name: &str, n: u64) {
        *self.counts.entry(name.into()).or_default() += n;
    }
    fn json(&self) -> Value {
        json!({"timings":self.timings.iter().map(|(k,v)|(k,v.json())).collect::<BTreeMap<_,_>>(),"counts":self.counts})
    }
}
#[derive(Default)]
struct Shared {
    all: Bucket,
    minute: Bucket,
    started: Option<Instant>,
    finished: bool,
    error: Option<String>,
    last_ui: Option<Instant>,
}
impl Shared {
    fn maximum(&mut self, name: &str, n: u64) {
        for bucket in [&mut self.all, &mut self.minute] {
            if let Some(old) = bucket.counts.get_mut(name) {
                *old = (*old).max(n);
            } else {
                bucket.counts.insert(name.into(), n);
            }
        }
    }
    fn timing(&mut self, name: &str, ms: f64) {
        self.all.timing(name, ms);
        self.minute.timing(name, ms);
    }
    fn count(&mut self, name: &str, n: u64) {
        self.all.count(name, n);
        self.minute.count(name, n);
    }
}
pub(super) fn tree_bytes(root: &Path) -> Value {
    fn walk(root: &Path, dir: &Path, rows: &mut BTreeMap<String, u64>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let ty = e.file_type().unwrap();
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                walk(root, &e.path(), rows);
            } else if ty.is_file() {
                rows.insert(
                    e.path().strip_prefix(root).unwrap().to_string_lossy().into_owned(),
                    e.metadata().unwrap().len(),
                );
            }
        }
    }
    let mut rows = BTreeMap::new();
    walk(root, root, &mut rows);
    json!({"files":rows.len(),"logical_bytes":rows.values().sum::<u64>(),"paths":rows})
}
fn query_probe(store: &mg_live::store::LiveStore, areas: &[mg_core::ResRef]) -> Value {
    use mg_live::events::Query;
    let mut queries = vec![
        ("all", Query::default()),
        ("chat", Query { kinds: vec!["chat".into(), "dialog".into()], ..Default::default() }),
        ("text", Query { text: Some("stress".into()), ..Default::default() }),
    ];
    queries.push(("area", Query { area: Some(areas[0].to_string()), ..Default::default() }));
    let mut rows = BTreeMap::new();
    for (name, q) in queries {
        let mut d = Distribution::default();
        let mut count = 0;
        for _ in 0..3 {
            let t = Instant::now();
            let result = store.events(&q);
            d.add(t.elapsed().as_secs_f64() * 1000.);
            count = result.0.len();
        }
        rows.insert(name, json!({"query":d.json(),"returned":count}));
    }
    json!(rows)
}

struct Soak {
    app: Moonglow,
    areas: Vec<mg_core::ResRef>,
    state: Arc<Mutex<Shared>>,
    ready: Option<std::sync::mpsc::Sender<()>>,
    output: PathBuf,
    data: PathBuf,
    minutes: File,
    startup: Value,
    metadata: Value,
    stage: u8,
    start: Instant,
    last_sample: Instant,
    cpu_start: f64,
    cpu_last: f64,
    counters: BTreeMap<String, (u64, u64, u64)>,
    last_rotate: u64,
    closing: bool,
    capture_phase: u8,
    effects_picture: bool,
    picture_path: Option<PathBuf>,
}
impl Soak {
    fn layout(&mut self) {
        // Two maps visible; remaining areas remain real open tabs. Tools share
        // three bottom panes; the right pane rotates Log/Panel/Selection.
        let mut dock = egui_dock::DockState::new(
            std::iter::once(Tab::Area(self.areas[0]))
                .chain(self.areas.iter().skip(2).map(|a| Tab::Area(*a)))
                .collect(),
        );
        let [maps, tools] = dock.main_surface_mut().split_below(
            egui_dock::NodeIndex::root(),
            0.61,
            vec![Tab::LiveChat],
        );
        dock.main_surface_mut().split_right(maps, 0.5, vec![Tab::Area(self.areas[1])]);
        let [chat, right] = dock.main_surface_mut().split_right(tools, 0.34, vec![Tab::LiveFocus]);
        let _ = chat;
        dock.main_surface_mut().split_right(
            right,
            0.5,
            vec![Tab::LiveLog, Tab::LivePanel, Tab::LiveSelection],
        );
        let floating = dock.add_window(vec![Tab::LiveChatArea(self.areas[0])]);
        if let Some(w) = dock.get_window_state_mut(floating) {
            w.set_position(egui::pos2(1350., 75.)).set_size(egui::vec2(530., 330.));
        }
        self.app.dock = dock;
        self.app.palette.area = Some(self.areas[0]);
    }
    fn sample(&mut self, final_sample: bool) {
        let now = Instant::now();
        let cpu = os::cpu();
        let store = self.app.live.as_ref().unwrap();
        let census_start = Instant::now();
        let census = mg_ui::measure::census_all(&self.app);
        let core = store.measure_census();
        let queries = query_probe(store, &self.areas);
        let files = tree_bytes(&self.data);
        let memory = os::memory();
        let overhead_ms = census_start.elapsed().as_secs_f64() * 1000.;
        let mut state = self.state.lock().unwrap();
        let elapsed = state.started.map_or(0., |t| t.elapsed().as_secs_f64());
        let interval = now.duration_since(self.last_sample).as_secs_f64();
        let row = json!({"elapsed_s":elapsed,"interval_s":interval,"cpu_one_core_percent":100.*(cpu-self.cpu_last)/interval,
            "memory":memory,"ui":census,"core":core,"files":files,"query_probe":queries,"sample_overhead_ms":overhead_ms,
            "minimized_os":os::minimized(),"metrics":state.minute.json(),"final":final_sample});
        serde_json::to_writer(&mut self.minutes, &row).unwrap();
        writeln!(self.minutes).unwrap();
        self.minutes.flush().unwrap();
        state.minute = Bucket::default();
        self.last_sample = now;
        self.cpu_last = cpu;
        if final_sample {
            let result = json!({"profile":"Release","duration_s":elapsed,"cpu_one_core_percent":100.*(cpu-self.cpu_start)/elapsed,
                "pid":std::process::id(),"startup":self.startup,"metadata":self.metadata,"metrics":state.all.json(),"last_sample":row,
                "error":state.error,"quantiles":"fixed 2048-bin logarithmic histogram; p50/p95 upper bound <=1%+1us; maxima exact",
                "method":"native eframe release, real resource/scene rendering; compressed captured traffic fed to an isolated store, no network/game sender; inclusive UI wall-time spans overlap; process CPU includes replay and driver threads"});
            std::fs::write(&self.output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        }
    }
}
impl eframe::App for Soak {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        for e in &input.events {
            if let egui::Event::Screenshot { image, .. } = e {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::save_buffer(
                    self.picture_path.take().unwrap_or_else(|| self.output.with_extension("png")),
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        input.events.clear();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.closing {
            return;
        }
        if self.stage == 0 {
            // Live previews are lazy. Visit all areas before measuring their retained caches.
            for area in &self.areas {
                if let Some(view) = self.app.area_views.get_mut(area) {
                    view.live.on = true;
                    view.live.effects = EffectsMode::Full;
                    // The stress fixture omits ARE lighting fields; its lit view is black.
                    // Use the normal unlit view setting so its geometry is inspectable.
                    view.lit = false;
                    if let Some(report) = self.app.live.as_ref().unwrap().area(&area.to_string()) {
                        let rows: Vec<_> = report.creatures.iter().take(50).collect();
                        if let Some(o) = &mut view.orbit {
                            if !rows.is_empty() {
                                o.target = rows
                                    .iter()
                                    .map(|o| glam::vec3(o.x, o.y, o.z))
                                    .sum::<glam::Vec3>()
                                    / rows.len() as f32;
                            }
                            o.distance = 65.;
                        }
                        if *area == self.areas[0] && view.live.selected.is_none() {
                            for (i, o) in rows.iter().enumerate() {
                                view.live.select(Some(o.id.clone()), i > 0);
                            }
                        }
                    }
                }
            }
            if let Some(area) = self
                .areas
                .iter()
                .find(|a| self.app.area_views.get(a).is_none_or(|v| v.live.report_reads == 0))
            {
                self.app.actions.push(Action::OpenTab(Tab::Area(*area)));
            } else {
                self.layout();
                self.stage = 1;
            }
        }
        let t = Instant::now();
        self.app.ui(ui);
        let ms = t.elapsed().as_secs_f64() * 1000.;
        let spans = mg_ui::measure::take();
        if self.startup.get("first_live_frame_ms").is_none()
            && self.app.area_views.values().any(|v| v.live.report_reads > 0)
        {
            self.startup["first_live_frame_ms"] =
                (self.start.elapsed().as_secs_f64() * 1000.).into();
        }
        if self.stage == 1
            && self
                .areas
                .iter()
                .take(2)
                .all(|a| self.app.area_views.get(a).is_some_and(|v| v.live.report_reads > 0))
        {
            self.startup["all_views_ready_ms"] =
                (self.start.elapsed().as_secs_f64() * 1000.).into();
            self.layout();
            self.stage = 2;
        } else if self.stage == 2 {
            // Let pending default Live-layout actions settle before fixing the stress layout.
            self.layout();
            self.stage = 3;
            if let Some(tx) = self.ready.take() {
                tx.send(()).unwrap();
            }
        }
        let mut s = self.state.lock().unwrap();
        if let Some(started) = s.started {
            s.last_ui = Some(Instant::now());
            if self.cpu_start == 0. {
                self.cpu_start = os::cpu();
                self.cpu_last = self.cpu_start;
                self.last_sample = Instant::now();
            }
            s.count("frames", 1);
            s.timing("ui", ms);
            for (name, (calls, ms)) in spans {
                s.timing(name, ms);
                s.count(&format!("calls:{name}"), calls);
            }
            for (area, v) in &self.app.area_views {
                let old = self.counters.entry(area.to_string()).or_default();
                if v.pictures > old.0 {
                    let visuals = mg_ui::measure::visual_counts(v);
                    for (name, count) in [
                        "max:effect_models_per_view",
                        "max:live_particles_per_view",
                        "max:cues_per_view",
                        "max:bubbles_per_view",
                    ]
                    .into_iter()
                    .zip(visuals)
                    {
                        s.maximum(name, count as u64);
                    }
                    if visuals[0] > 0 && !self.effects_picture && self.picture_path.is_none() {
                        self.effects_picture = true;
                        self.picture_path = Some(self.output.with_extension("effects.png"));
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(
                            Default::default(),
                        ));
                    }
                }
                s.count("pictures", v.pictures.saturating_sub(old.0));
                s.count("report_reads", v.live.report_reads.saturating_sub(old.1));
                s.count("reconciliations", v.live.reconciliations.saturating_sub(old.2));
                *old = (v.pictures, v.live.report_reads, v.live.reconciliations);
            }
            for cause in ui.ctx().repaint_causes() {
                s.count(&format!("repaint:{cause}"), 1);
            }
            if ui.input(|i| i.viewport().minimized.unwrap_or(false)) {
                s.count("minimized_frames", 1);
            }
            if (self.capture_phase == 0 && started.elapsed().as_secs() >= 5)
                || (self.capture_phase == 1 && started.elapsed().as_secs() >= 105)
            {
                self.picture_path = Some(self.output.with_extension(if self.capture_phase == 0 {
                    "png"
                } else {
                    "group.png"
                }));
                self.capture_phase += 1;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let slot = started.elapsed().as_secs() / 20;
            if slot != self.last_rotate {
                self.last_rotate = slot;
                let tab =
                    [Tab::LiveLog, Tab::LivePanel, Tab::LiveSelection][slot as usize % 3].clone();
                if let Some(at) = self.app.dock.find_tab(&tab) {
                    let _ = self.app.dock.set_active_tab(at);
                }
                self.app.live_chat.area = if slot % 2 == 0 {
                    None
                } else {
                    Some(self.areas[slot as usize % self.areas.len()].to_string())
                };
                self.app.live_focus.selected =
                    Some(slot as usize % self.app.live_focus.entries.len().max(1));
            }
        } else {
            for (a, v) in &self.app.area_views {
                self.counters.insert(
                    a.to_string(),
                    (v.pictures, v.live.report_reads, v.live.reconciliations),
                );
            }
        }
        let finished = s.finished;
        drop(s);
        if finished && self.picture_path.is_some() {
            ui.ctx().request_repaint();
        } else if finished {
            self.sample(true);
            self.closing = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.cpu_start != 0. && self.last_sample.elapsed() >= Duration::from_secs(60) {
            self.sample(false);
        }
        if self.stage < 3 {
            ui.ctx().request_repaint();
        }
    }
}

pub(super) fn run() -> eframe::Result<()> {
    let start = Instant::now();
    let path = PathBuf::from(std::env::var("MGLIVE_STRESS_REPLAY").unwrap());
    let output = PathBuf::from(std::env::var("MGLIVE_MEASURE_OUT").unwrap());
    let module =
        PathBuf::from(std::env::var("MGLIVE_STRESS_MODULE").expect("explicit replay module"));
    let data = PathBuf::from(std::env::var("MGLIVE_STRESS_DATA").expect("isolated data folder"));
    let root = PathBuf::from(std::env::var("NWN_ROOT").unwrap());
    let user = PathBuf::from(std::env::var("NWN_HOME").unwrap());
    let seconds = std::env::var("MGLIVE_MEASURE_SECONDS").unwrap().parse::<u64>().unwrap();
    let restored = std::env::var_os("MGLIVE_STRESS_RESTORE").is_some();
    let side = std::env::var_os("MGLIVE_STRESS_DRAFTS").map(PathBuf::from);
    let reject_only = std::env::var_os("MGLIVE_STRESS_REJECT_ONLY").is_some();
    let index = replay::inspect(&path, side.as_deref());
    let scan_ms = start.elapsed().as_secs_f64() * 1000.;
    assert!(index.areas.len() >= 2, "stress layout needs at least two native areas");
    let areas: Vec<mg_core::ResRef> = index.areas.iter().map(|a| a.parse().unwrap()).collect();
    let store = Arc::new(mg_live::store::LiveStore::default());
    store.assistant_context(data.clone(), module.clone(), "127.0.0.1:6379:mglive".into()).unwrap();
    let mut prime = replay::Stream::open(&path, side.as_deref());
    let mut primed_until = 0;
    while let Some(row) = prime.next() {
        match row.channel.as_str() {
            "state" => {
                store.sync(&serde_json::to_vec(&row.data).unwrap(), mg_live::now_ms()).unwrap();
            }
            "event" => {
                store
                    .measure_event_packet(
                        &serde_json::to_vec(&row.data).unwrap(),
                        mg_live::now_ms(),
                    )
                    .unwrap();
            }
            _ => {}
        }
        primed_until = row.at_ms;
        if !restored && areas.iter().all(|a| store.area(&a.to_string()).is_some()) {
            break;
        }
    }
    let prime_ms = start.elapsed().as_secs_f64() * 1000. - scan_ms;
    let state = Arc::new(Mutex::new(Shared::default()));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1920., 1080.])
            .with_title("Moonglow GUI stress replay (isolated)")
            .with_active(false),
        persist_window: false,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(egui_wgpu::WgpuSetupCreateNew {
                device_descriptor: Arc::new(|a| wgpu::DeviceDescriptor {
                    required_features: a.features() & wgpu::Features::TEXTURE_COMPRESSION_BC,
                    required_limits: a.limits(),
                    ..Default::default()
                }),
                ..egui_wgpu::WgpuSetupCreateNew::without_display_handle()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Moonglow stress measurement",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.disable_accesskit();
            let resource_start = Instant::now();
            let mut app = Moonglow::new(
                Some(mg_resman::GameInstall::new(&root, Some(user), "en")),
                Box::new(NoDialogs::default()),
            );
            let resources_ms = resource_start.elapsed().as_secs_f64() * 1000.;
            app.no_plugins = true;
            app.recovery_dir = None;
            app.compiled_dir = None;
            app.open_palette = false;
            app.live_assistant.data_dir = Some(data.clone());
            app.live_focus.data_dir = Some(data.clone());
            app.live_scene.data_dir = Some(data.clone());
            app.set_render_state(cc.wgpu_render_state.clone().unwrap());
            let t = Instant::now();
            app.open_module(&module);
            let module_ms = t.elapsed().as_secs_f64() * 1000.;
            app.live = Some(store.clone());
            app.settings.live.enabled = true;
            app.live_focus.context(app.module_path());
            let mut watched = Vec::new();
            let mut group = Vec::new();
            for area in &areas {
                let report = store.area(&area.to_string()).unwrap();
                let mut ids = index.speakers.get(&area.to_string()).cloned().unwrap_or_default();
                ids.retain(|id| report.creatures.iter().any(|o| &o.id == id));
                for o in &report.creatures {
                    if !ids.contains(&o.id) {
                        ids.push(o.id.clone());
                    }
                }
                if let Some(id) = ids.first() {
                    watched.push(id.clone());
                }
                group.extend(ids.into_iter().take(10));
            }
            group.truncate(50);
            assert!(watched.len() >= 5, "five watched native entries required");
            if !restored {
                app.live_focus.entries.clear();
            }
            if app.live_focus.entries.is_empty() {
                for id in watched.iter().take(5) {
                    app.live_focus.add(&store, std::slice::from_ref(id)).unwrap();
                }
                app.live_focus.add(&store, &group).unwrap();
            }
            for id in watched.iter().take(5) {
                if let Ok(mut card) = store.npc_card(id)
                    && card.who.is_empty()
                {
                    card.who = "Native stress fixture NPC; measurement guidance only".into();
                    store.save_npc_card(id, card).unwrap();
                }
            }
            for id in &group {
                if store.object_note(id).is_ok_and(|s| s.is_empty()) {
                    store.save_object_note(id, "Stress measurement watch note").unwrap();
                }
            }
            for area in &areas {
                app.actions.push(Action::OpenTab(Tab::Area(*area)));
            }
            let state2 = state.clone();
            let store2 = store.clone();
            let ctx = cc.egui_ctx.clone();
            let metadata = json!({"replay":path,"module":module,"restored":restored,"input":index.summary,"gpu":format!("{:?}",cc.wgpu_render_state.as_ref().unwrap().adapter.get_info()),"view_points":[1920,1080],"primed_until_ms":primed_until,"drafts":side,"draft_reject_only_control":reject_only,"unlit_view":true,"watched":watched,"group":group});
            let driver_output = output.with_extension("driver.json");
            std::thread::spawn(move || {
                ready_rx.recv_timeout(Duration::from_secs(180)).expect("stress first Live frames");
                std::thread::sleep(Duration::from_secs(2));
                let wall = Instant::now();
                state2.lock().unwrap().started = Some(wall);
                let replay_client = store2.measure_replay_client(&index.client);
                let mut pre_baseline = std::collections::HashSet::new();
                let mut cycle = 0u64;
                'cycles: loop {
                    let mut reader = replay::Stream::open(&path, side.as_deref());
                    while let Some(mut row) = reader.next() {
                        if cycle == 0 && row.at_ms <= primed_until {
                            if row.channel == "draft"
                                && let Some(id) = row.data["command"]["id"].as_str()
                            {
                                pre_baseline.insert(id.to_owned());
                            }
                            if row.channel == "decision"
                                && let Some(id) = row.data["id"].as_str()
                            {
                                pre_baseline.remove(id);
                            }
                            continue;
                        }
                        if row.channel == "decision"
                            && row.data["id"].as_str().is_some_and(|id| pre_baseline.remove(id))
                        {
                            state2.lock().unwrap().count("omitted_pre_baseline_decisions", 1);
                            continue;
                        }
                        if reject_only && row.channel == "decision" {
                            row.data["text"] = Value::Null;
                        }
                        let target_ms = (cycle * index.duration_ms
                            + row.at_ms.saturating_sub(index.first_ms))
                        .saturating_sub(primed_until.saturating_sub(index.first_ms));
                        if target_ms >= seconds * 1000 {
                            break 'cycles;
                        }
                        while wall.elapsed().as_millis() < (target_ms as u128) {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        let t = Instant::now();
                        replay::offset(&mut row, cycle, &index);
                        if row.channel == "state" {
                            replay_client.translate_results(&mut row.data);
                        }
                        let result = match row.channel.as_str() {
                            "state" => store2
                                .sync(&serde_json::to_vec(&row.data).unwrap(), mg_live::now_ms())
                                .map(|_| true),
                            "event" => store2.measure_event_packet(
                                &serde_json::to_vec(&row.data).unwrap(),
                                mg_live::now_ms(),
                            ),
                            "draft" => replay_client
                                .submit(&store2, row.data.clone(), mg_live::now_ms())
                                .map(|_| true),
                            "decision" => replay_client
                                .decision(&store2, row.data.clone(), mg_live::now_ms())
                                .map(|_| true),
                            "marker" => Ok(false),
                            "log" => {
                                store2.server_log(&row.data, mg_live::now_ms());
                                Ok(true)
                            }
                            _ => Err(format!("Unsupported replay channel {}", row.channel)),
                        };
                        let mut s = state2.lock().unwrap();
                        s.timing(
                            &format!("ingest.{}", row.channel),
                            t.elapsed().as_secs_f64() * 1000.,
                        );
                        s.timing(
                            "replay.lag",
                            wall.elapsed().as_secs_f64() * 1000. - target_ms as f64,
                        );
                        s.count(&format!("input.{}", row.channel), 1);
                        let wake = match result {
                            Ok(wake) => wake,
                            Err(e) => {
                                s.error = Some(e);
                                break 'cycles;
                            }
                        };
                        drop(s);
                        if wake {
                            mg_ui::live_repaint::wake(&ctx);
                        }
                    }
                    cycle += 1;
                    state2.lock().unwrap().count("replay_cycles", 1);
                }
                while wall.elapsed() < Duration::from_secs(seconds)
                    && state2.lock().unwrap().error.is_none()
                {
                    std::thread::sleep(Duration::from_millis(100));
                }
                if let Err(e) = store2.flush_npc_memory() {
                    state2.lock().unwrap().error = Some(e);
                }
                {
                    let mut s = state2.lock().unwrap();
                    s.finished = true;
                    let checkpoint = json!({"elapsed_s":wall.elapsed().as_secs_f64(),"error":s.error,
                        "last_ui_ago_s":s.last_ui.map(|t|t.elapsed().as_secs_f64()),"counts":s.all.counts,
                        "minimized_os":os::minimized(),"cpu_seconds":os::cpu()});
                    std::fs::write(driver_output, serde_json::to_vec(&checkpoint).unwrap())
                        .unwrap();
                }
                ctx.request_repaint();
                // A minimized diagnostic cannot rely on another UI pass. Never leave it behind.
                std::thread::sleep(Duration::from_secs(5));
                std::process::exit(2);
            });
            let minutes = File::create(output.with_extension("minutes.jsonl")).unwrap();
            Ok(Box::new(Soak {
                app,
                areas,
                state,
                ready: Some(ready_tx),
                output,
                data,
                minutes,
                startup: json!({"scan_ms":scan_ms,"prime_ms":prime_ms,"resources_ms":resources_ms,"module_ms":module_ms}),
                metadata,
                stage: 0,
                start,
                last_sample: Instant::now(),
                cpu_start: 0.,
                cpu_last: 0.,
                counters: BTreeMap::new(),
                last_rotate: 0,
                closing: false,
                capture_phase: 0,
                effects_picture: false,
                picture_path: None,
            }))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_drafts_use_real_scope_and_translate_recorded_acknowledgements() {
        let store = mg_live::store::LiveStore::default();
        store.sync(br#"{"v":1,"boot":"fixture","seq":1,"areas":[{"area":"room","full":true,"cre":[{"id":"1","name":"Guard"}]}]}"#,100).unwrap();
        let client = store.measure_replay_client("Replay conversation");
        let draft=client.submit(&store,json!({"id":"logical","command":{"area":"room","op":"npc_say","target":"1","text":"Hello"}}),101).unwrap()["queued"].as_str().unwrap().to_owned();
        assert!(store.commands(102)["commands"].as_array().unwrap().is_empty());
        client
            .decision(
                &store,
                json!({"id":"logical","text":"Edited","native_id":"captured-native"}),
                103,
            )
            .unwrap();
        let mut report = json!({"v":1,"boot":"fixture","seq":2,"areas":[],"ack":["captured-native"],"results":[{"id":"captured-native","ok":true}]});
        client.translate_results(&mut report);
        assert_ne!(report["ack"][0], "captured-native");
        store.sync(&serde_json::to_vec(&report).unwrap(), 104).unwrap();
        assert_eq!(store.result(&draft).unwrap().value.unwrap()["draft"]["outcome"], "edited");
        assert_eq!(store.measure_census()["assistant"]["sending"], 0);
        assert!(
            client
                .submit(
                    &store,
                    json!({"id":"forbidden","command":{"area":"room","op":"kill","target":"1"}}),
                    105
                )
                .is_err()
        );
    }
}
