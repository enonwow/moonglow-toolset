//! Data-driven Live wakes and deadlines, without an extra settling pass.
use std::time::Duration;

/// Accepted report/log/connection changes need one UI pass. egui coalesces
/// multiple wakes before that pass; ordinary input keeps its normal repaint rules.
#[track_caller]
pub fn wake(ctx: &egui::Context) {
    after(ctx, Duration::ZERO);
}

/// Schedule a real deadline from now. egui subtracts predicted_dt from delayed
/// requests and reserves a second pass for a zero argument. Compensate for that
/// lead time, and keep the original argument nonzero even for an immediate wake.
#[track_caller]
pub(crate) fn after(ctx: &egui::Context, delay: Duration) {
    let lead = ctx.input(|i| Duration::try_from_secs_f32(i.predicted_dt).unwrap_or_default());
    ctx.request_repaint_after(delay.saturating_add(lead).max(Duration::from_nanos(1)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn settled() -> egui::Context {
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(Default::default(), |_| {}).drop_without_applying_deltas();
        }
        ctx
    }

    #[test]
    fn report_wakes_coalesce_without_a_second_pass() {
        let ctx = settled();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let sink = calls.clone();
        ctx.set_request_repaint_callback(move |info| sink.lock().unwrap().push(info.delay));
        for _ in 0..8 {
            wake(&ctx);
        }
        assert_eq!(*calls.lock().unwrap(), [Duration::ZERO]);
        let output = ctx.run_ui(Default::default(), |_| {});
        assert_eq!(output.viewport_output[&egui::ViewportId::ROOT].repaint_delay, Duration::MAX);
        output.drop_without_applying_deltas();
        assert_eq!(calls.lock().unwrap().len(), 1);
        wake(&ctx);
        assert_eq!(calls.lock().unwrap().len(), 2, "the next report still wakes the view");
    }

    #[test]
    fn deadline_is_not_advanced_by_the_predicted_frame_time() {
        let ctx = settled();
        let output =
            ctx.run_ui(Default::default(), |ui| after(ui.ctx(), Duration::from_millis(40)));
        assert_eq!(
            output.viewport_output[&egui::ViewportId::ROOT].repaint_delay,
            Duration::from_millis(40)
        );
        output.drop_without_applying_deltas();
    }
}
