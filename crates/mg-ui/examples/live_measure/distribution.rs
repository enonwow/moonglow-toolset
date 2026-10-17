//! Bounded streaming timings. Quantiles are bucket upper bounds (<=1% +1us).
use serde_json::{Value, json};

#[derive(Clone)]
pub(super) struct Distribution {
    bins: Vec<u64>,
    count: u64,
    sum: f64,
    max: f64,
}
impl Default for Distribution {
    fn default() -> Self {
        Self { bins: vec![0; 2048], count: 0, sum: 0., max: 0. }
    }
}
impl Distribution {
    pub(super) fn add(&mut self, milliseconds: f64) {
        assert!(milliseconds.is_finite() && milliseconds >= 0.);
        let bin = ((milliseconds * 1000.).ln_1p() / 1.01_f64.ln()).floor() as usize;
        self.bins[bin.min(2047)] += 1;
        self.count += 1;
        self.sum += milliseconds;
        self.max = self.max.max(milliseconds);
    }
    fn quantile(&self, fraction: f64) -> f64 {
        let wanted = (self.count as f64 * fraction).ceil() as u64;
        let mut n = 0;
        for (i, count) in self.bins.iter().enumerate() {
            n += count;
            if n >= wanted {
                return (((i + 1) as f64 * 1.01_f64.ln()).exp_m1() / 1000.).min(self.max);
            }
        }
        self.max
    }
    pub(super) fn json(&self) -> Value {
        if self.count == 0 {
            return json!({"n":0});
        }
        json!({"n":self.count,"mean_ms":self.sum/self.count as f64,
            "p50_ms":self.quantile(0.50),"p95_ms":self.quantile(0.95),"max_ms":self.max})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_quantiles_bound_the_actual_percentile_without_growing() {
        let mut d = Distribution::default();
        for n in 1..=100_000 {
            d.add(f64::from(n) / 1000.);
        }
        assert_eq!(d.bins.len(), 2048);
        assert_eq!(d.count, 100_000);
        for (fraction, expected) in [(0.5, 50.), (0.95, 95.)] {
            let q = d.quantile(fraction);
            assert!(q >= expected && q <= expected * 1.01 + 0.001, "{q}");
        }
        assert_eq!(d.max, 100.);
    }
}
