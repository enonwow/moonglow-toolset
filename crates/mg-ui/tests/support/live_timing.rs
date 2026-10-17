//! Same UI/tessellation/fence timing boundary as frame_perf, on a replayed report.
use egui_kittest::Harness;
use mg_ui::{Moonglow, live_effects::EffectsMode};
use std::time::{Duration, Instant};

pub(crate) fn measure(h: &mut Harness<'_, Moonglow>, area: mg_core::ResRef) -> serde_json::Value {
    let ctx = h.ctx.clone();
    ctx.disable_accesskit();
    let device = h.state().viewport.as_ref().unwrap().gpu.device.clone();
    let wait = || {
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
    };
    let mut modes = Vec::new();
    for mode in [EffectsMode::Markers, EffectsMode::Full] {
        h.state_mut().area_views.get_mut(&area).unwrap().live.effects = mode;
        let mut cpu = Vec::new();
        let mut gpu = Vec::new();
        let mut prepare = Vec::new();
        let mut poses = Vec::new();
        let reads = h.state().area_views[&area].live.report_reads;
        let reconciliations = h.state().area_views[&area].live.reconciliations;
        let mut clock = ctx.input(|i| i.time);
        for n in 0..144 {
            clock += 1. / 144.;
            let mut input = h.input().clone();
            input.time = Some(clock);
            input.predicted_dt = 1. / 144.;
            input.events.clear();
            wait();
            let start = Instant::now();
            let mut full = ctx.run_ui(input, |ui| {
                ui.scope_builder(egui::UiBuilder::new(), |ui| {
                    egui::Frame::central_panel(ui.style())
                        .outer_margin(8.)
                        .inner_margin(0.)
                        .show(ui, |ui| h.state_mut().ui(ui));
                });
            });
            let _shapes = ctx.tessellate(full.shapes, full.pixels_per_point);
            // Like frame_perf, this measures the 3D submission and CPU UI pass,
            // not the final egui paint. Explicitly discard unsubmitted font deltas.
            full.textures_delta.clear();
            let submitted = Instant::now();
            wait();
            if n >= 24 {
                let times = h.state().area_views[&area].live.frame_cpu_ms;
                prepare.push(times[0]);
                poses.push(times[1]);
                cpu.push((submitted - start).as_secs_f64() * 1000.);
                gpu.push(submitted.elapsed().as_secs_f64() * 1000.);
            }
        }
        let stats = |mut values: Vec<f64>| {
            values.sort_by(f64::total_cmp);
            serde_json::json!({"median_ms":values[values.len()/2],"p95_ms":values[values.len()*95/100],"max_ms":values[values.len()-1]})
        };
        modes.push(serde_json::json!({"mode":mode.name(),"live_prepare":stats(prepare),"live_poses":stats(poses),"report_reads_including_warmup":h.state().area_views[&area].live.report_reads-reads,"reconciliations_including_warmup":h.state().area_views[&area].live.reconciliations-reconciliations,"cpu_ui_and_tessellation":stats(cpu),"gpu_fence_after_submission":stats(gpu),"live_particles":h.state().area_views[&area].live_particles_shown}));
    }
    ctx.enable_accesskit();
    serde_json::json!({"profile":"guarded UI opt-level 0, no UI debug; dependencies unchanged", "screen":[1600,1000],"frames_per_mode":120,"warmup_per_mode":24,"clock_hz":144,"cpu_budget_ms":6.944,"concurrent_load":"uncontrolled; protected demo is not stopped or rebuilt by this harness","modes":modes})
}
