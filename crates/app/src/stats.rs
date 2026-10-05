//! AKSHAT_FRAME_STATS=1 records how long every frame takes, and prints a
//! summary each time the window hides. Off, it costs one branch per frame.
//!
//! Two numbers per frame. Build is the time spent in our own render code.
//! Interval is the gap since the previous frame, counted only when frames
//! come back to back (an animation, a scroll, typing), where it's the whole
//! frame: layout, paint and the gpu (or the cpu doing the gpu's job).

use std::time::{Duration, Instant};

// Frames further apart than this weren't part of one continuous run.
const CONTINUOUS: Duration = Duration::from_millis(100);

pub struct FrameStats {
    on: bool,
    last: Option<Instant>,
    build: Vec<f32>,
    interval: Vec<f32>,
}

impl FrameStats {
    pub fn new() -> Self {
        Self {
            on: std::env::var_os("AKSHAT_FRAME_STATS").is_some(),
            last: None,
            build: Vec::new(),
            interval: Vec::new(),
        }
    }

    /// Call at the start of a frame, returns when it started.
    pub fn begin(&mut self) -> Option<Instant> {
        if !self.on {
            return None;
        }
        let now = Instant::now();
        if let Some(gap) = self.last.map(|l| now - l).filter(|g| *g < CONTINUOUS) {
            self.interval.push(gap.as_secs_f32() * 1000.0);
        }
        self.last = Some(now);
        Some(now)
    }

    pub fn end(&mut self, started: Option<Instant>) {
        if let Some(t) = started {
            self.build.push(t.elapsed().as_secs_f32() * 1000.0);
        }
    }

    pub fn report(&mut self) {
        if !self.on || self.build.is_empty() {
            return;
        }
        let pct = |v: &mut Vec<f32>, p: f32| {
            v.sort_by(f32::total_cmp);
            v.get(((v.len() - 1) as f32 * p) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let (b50, b95, bmax) = (
            pct(&mut self.build, 0.5),
            pct(&mut self.build, 0.95),
            pct(&mut self.build, 1.0),
        );
        eprintln!(
            "frames {}  build ms p50 {b50:.2} p95 {b95:.2} max {bmax:.2}",
            self.build.len()
        );
        if !self.interval.is_empty() {
            let (i50, i95) = (pct(&mut self.interval, 0.5), pct(&mut self.interval, 0.95));
            eprintln!(
                "continuous {}  frame ms p50 {i50:.2} p95 {i95:.2}  ({:.0} fps typical, {:.0} fps at the slow end)",
                self.interval.len(),
                1000.0 / i50,
                1000.0 / i95
            );
        }
        self.build.clear();
        self.interval.clear();
        self.last = None;
    }
}
