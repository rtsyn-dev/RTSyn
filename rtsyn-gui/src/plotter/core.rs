use crate::plotter::data::{PlotSeries, SeriesMinMax, SeriesTransform};
use crate::plotter::{palette_color, transform_value, MAX_SERIES};
use std::collections::VecDeque;

pub struct LivePlotter {
    pub(crate) plugin_id: u64,
    pub(crate) open: bool,
    pub(crate) input_count: usize,
    pub(crate) refresh_hz: f64,
    pub(crate) window_ms: f64,
    pub(crate) max_points: usize,
    pub(crate) max_points_effective: usize,
    pub(crate) bucket_size: u64,
    pub(crate) bucket_count: u64,
    pub(crate) bucket_minmax: Vec<SeriesMinMax>,
    pub(crate) last_tick: Option<u64>,
    pub(crate) last_time_s: Option<f64>,
    pub(crate) last_time_x: Option<f64>,
    pub(crate) last_time_scale: f64,
    pub(crate) series: Vec<PlotSeries>,
    pub(crate) raw_series: Vec<VecDeque<(f64, f64)>>,
}

impl LivePlotter {
    pub(crate) fn new(plugin_id: u64) -> Self {
        Self {
            plugin_id,
            open: false,
            input_count: 0,
            refresh_hz: 60.0,
            window_ms: 10_000.0,
            max_points: 200,
            max_points_effective: 200,
            bucket_size: 1,
            bucket_count: 0,
            bucket_minmax: Vec::new(),
            last_tick: None,
            last_time_s: None,
            last_time_x: None,
            last_time_scale: 1000.0,
            series: Vec::new(),
            raw_series: Vec::new(),
        }
    }

    pub(crate) fn update_config(&mut self, input_count: usize, refresh_hz: f64, period_s: f64) {
        self.input_count = input_count.min(MAX_SERIES);
        self.refresh_hz = if refresh_hz <= 0.0 { 60.0 } else { refresh_hz };
        let period_s = if period_s <= 0.0 { 0.0 } else { period_s };
        let expected_points = if period_s > 0.0 {
            (self.window_ms / (period_s * 1000.0)).ceil() as usize
        } else {
            0
        };
        const MIN_POINTS: usize = 200;
        const TARGET_POINTS: usize = 12_000;
        self.max_points = if expected_points == 0 {
            MIN_POINTS
        } else {
            expected_points.clamp(MIN_POINTS, TARGET_POINTS)
        };
        self.bucket_size = if expected_points > self.max_points && self.max_points > 0 {
            ((expected_points + self.max_points - 1) / self.max_points) as u64
        } else {
            1
        };
        self.bucket_size = self.bucket_size.max(1);
        self.bucket_count = 0;
        self.max_points_effective = if self.bucket_size > 1 {
            self.max_points.saturating_mul(2)
        } else {
            self.max_points
        };
        if self.series.len() != self.input_count {
            self.series = (0..self.input_count)
                .map(|idx| PlotSeries::new(format!("in_{idx}"), palette_color(idx)))
                .collect();
        }
        if self.raw_series.len() != self.input_count {
            self.raw_series = vec![VecDeque::new(); self.input_count];
        }
        if self.bucket_minmax.len() != self.input_count {
            self.bucket_minmax = vec![SeriesMinMax::default(); self.input_count];
        }
    }

    pub(crate) fn set_window_ms(&mut self, window_ms: f64) {
        self.window_ms = if window_ms <= 0.0 { 1.0 } else { window_ms };
    }

    pub(crate) fn set_series_names(&mut self, names: Vec<String>) {
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (idx, name) in names.into_iter().enumerate() {
            if let Some(series) = self.series.get_mut(idx) {
                let entry = counts.entry(name.clone()).or_insert(0);
                *entry += 1;
                if *entry == 1 {
                    series.name = name;
                } else {
                    series.name = format!("{name} ({})", *entry);
                }
            }
        }
    }

    pub(crate) fn push_sample(&mut self, tick: u64, time_s: f64, time_scale: f64, values: &[f64]) {
        if self.last_tick == Some(tick) {
            return;
        }
        self.last_tick = Some(tick);
        self.last_time_s = Some(time_s);
        let time_x = time_s * time_scale;
        if let (Some(_prev_time), prev_scale) = (self.last_time_x, self.last_time_scale) {
            let scale_shift = (prev_scale - time_scale).abs() > f64::EPSILON;
            if scale_shift {
                for series in &mut self.series {
                    series.points.clear();
                }
                for raw_series in &mut self.raw_series {
                    raw_series.clear();
                }
                self.bucket_count = 0;
                for entry in &mut self.bucket_minmax {
                    *entry = SeriesMinMax::default();
                }
            }
        }
        self.last_time_x = Some(time_x);
        self.last_time_scale = time_scale;

        for (idx, value) in values.iter().copied().enumerate() {
            if let Some(raw_series) = self.raw_series.get_mut(idx) {
                raw_series.push_back((time_x, value));
            }
        }

        if self.bucket_size == 1 {
            for (idx, value) in values.iter().copied().enumerate() {
                if let Some(series) = self.series.get_mut(idx) {
                    series.points.push_back((time_x, value));
                }
            }
        } else {
            for (idx, value) in values.iter().copied().enumerate() {
                if let Some(entry) = self.bucket_minmax.get_mut(idx) {
                    let next = (time_x, value);
                    entry.min = Some(match entry.min {
                        Some(prev) if prev.1 <= value => prev,
                        _ => next,
                    });
                    entry.max = Some(match entry.max {
                        Some(prev) if prev.1 >= value => prev,
                        _ => next,
                    });
                }
            }
            self.bucket_count += 1;
            if self.bucket_count >= self.bucket_size {
                self.bucket_count = 0;
                for (idx, entry) in self.bucket_minmax.iter_mut().enumerate() {
                    if let Some(series) = self.series.get_mut(idx) {
                        let min = entry.min.take();
                        let max = entry.max.take();
                        match (min, max) {
                            (Some(a), Some(b)) => {
                                if a.0 <= b.0 {
                                    series.points.push_back(a);
                                    if a.0 != b.0 || a.1 != b.1 {
                                        series.points.push_back(b);
                                    }
                                } else {
                                    series.points.push_back(b);
                                    if a.0 != b.0 || a.1 != b.1 {
                                        series.points.push_back(a);
                                    }
                                }
                            }
                            (Some(a), None) | (None, Some(a)) => {
                                series.points.push_back(a);
                            }
                            (None, None) => {}
                        }
                    }
                }
            }
        }
        self.prune_old(time_x, time_scale);
    }

    pub(crate) fn push_sample_from_tick(
        &mut self,
        tick: u64,
        period_s: f64,
        time_scale: f64,
        values: &[f64],
    ) {
        let period_s = period_s.max(0.0);
        let time_s = match (self.last_tick, self.last_time_s) {
            (Some(prev_tick), Some(prev_time_s)) if tick >= prev_tick => {
                prev_time_s + (tick - prev_tick) as f64 * period_s
            }
            _ => tick as f64 * period_s,
        };
        self.push_sample(tick, time_s, time_scale, values);
    }

    pub(crate) fn flush_pending_bucket(&mut self) {
        if self.bucket_size <= 1 || self.bucket_count == 0 {
            return;
        }
        self.bucket_count = 0;
        for (idx, entry) in self.bucket_minmax.iter_mut().enumerate() {
            if let Some(series) = self.series.get_mut(idx) {
                let min = entry.min.take();
                let max = entry.max.take();
                match (min, max) {
                    (Some(a), Some(b)) => {
                        if a.0 <= b.0 {
                            series.points.push_back(a);
                            if a.0 != b.0 || a.1 != b.1 {
                                series.points.push_back(b);
                            }
                        } else {
                            series.points.push_back(b);
                            if a.0 != b.0 || a.1 != b.1 {
                                series.points.push_back(a);
                            }
                        }
                    }
                    (Some(a), None) | (None, Some(a)) => {
                        series.points.push_back(a);
                    }
                    (None, None) => {}
                }
            }
        }
    }

    fn prune_old(&mut self, now_x: f64, time_scale: f64) {
        let window_units = (self.window_ms * time_scale / 1000.0).max(0.000_001);
        let min_time = now_x - window_units;
        for series in &mut self.series {
            while let Some((t, _)) = series.points.front().copied() {
                if t >= min_time {
                    break;
                }
                series.points.pop_front();
            }
            while series.points.len() > self.max_points_effective {
                series.points.pop_front();
            }
        }
        const RAW_MAX_POINTS: usize = 300_000;
        for raw_series in &mut self.raw_series {
            while let Some((t, _)) = raw_series.front().copied() {
                if t >= min_time {
                    break;
                }
                raw_series.pop_front();
            }
            while raw_series.len() > RAW_MAX_POINTS {
                raw_series.pop_front();
            }
        }
    }

    pub(crate) fn compute_bounds(
        &self,
        custom_series_transforms: Option<&[SeriesTransform]>,
        custom_window_ms: Option<f64>,
    ) -> (f64, f64, f64, f64) {
        let last_time = match self.last_time_x {
            Some(value) => value,
            None => return (f64::NAN, f64::NAN, f64::NAN, f64::NAN),
        };
        let window_ms = custom_window_ms.unwrap_or(self.window_ms).max(0.000_001);
        let window_units = (window_ms * self.last_time_scale / 1000.0).max(0.000_001);
        let min_time = last_time - window_units;
        let max_time = last_time;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for (series_idx, series) in self.series.iter().enumerate() {
            for (t, y) in &series.points {
                if *t < min_time || *t > max_time {
                    continue;
                }
                let transformed =
                    transform_value(*y, series_idx, custom_series_transforms).unwrap_or(*y);
                if transformed < min_y {
                    min_y = transformed;
                }
                if transformed > max_y {
                    max_y = transformed;
                }
            }
        }
        if min_y.is_infinite() || max_y.is_infinite() {
            return (min_time, max_time, -1.0, 1.0);
        }
        if min_y == max_y {
            min_y -= 1.0;
            max_y += 1.0;
        } else {
            let pad = (max_y - min_y) * 0.05;
            min_y -= pad;
            max_y += pad;
        }
        (min_time, max_time, min_y, max_y)
    }
}

#[cfg(test)]
mod tests {
    use super::LivePlotter;

    #[test]
    fn high_rate_window_uses_bucketing() {
        let mut plotter = LivePlotter::new(1);
        plotter.set_window_ms(50_000.0);
        plotter.update_config(1, 60.0, 0.0001); // 100 us

        assert!(plotter.bucket_size > 1);
        assert!(plotter.max_points_effective <= 24_000);
    }
}