//! Motion from decoded pictures, without knowing anything about video:
//! the luma plane is reduced to a coarse grid of cell brightness, compared
//! with a slowly adapting background, and the changed area (inside the
//! zones) decides whether there is motion. Hysteresis turns the per-picture
//! verdicts into clean start/end transitions.

use std::time::Duration;

use watchgrid_model::MotionZone;

/// Grid resolution, independent of the stream's size (16:9 friendly).
pub const GRID_W: usize = 32;
pub const GRID_H: usize = 18;

/// Pictures used to learn the scene before anything can trigger.
const WARMUP: u32 = 10;
/// Background adaptation per analysed picture (≈ 5 per second).
const LEARN: f32 = 0.05;
/// Slower adaptation where something moves, so a person standing still
/// isn't absorbed into the background at once.
const LEARN_MOVING: f32 = 0.01;
/// A uniform brightness shift (clouds, a lamp, exposure) is subtracted
/// before comparing. If this much of the watched area still differs, the
/// scene changed as a whole (IR switching on or off): relearn instead of
/// triggering.
const LIGHT_CHANGE: f32 = 0.6;
/// Consecutive motion pictures needed to start (filters single-frame noise).
const START_HITS: u32 = 2;
/// Quiet time before a detection ends.
pub const END_AFTER: Duration = Duration::from_secs(3);

/// How strict the detector is, from the camera's 0-100 sensitivity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Brightness difference (0-255) for a cell to count as changed.
    pub cell_delta: f32,
    /// Share of the watched cells that must change.
    pub min_area: f32,
}

impl Thresholds {
    pub fn from_sensitivity(sensitivity: u8) -> Self {
        let s = f32::from(sensitivity.min(100));
        Self { cell_delta: 6.0 + (100.0 - s) * 0.3, min_area: 0.02 - 0.00018 * s }
    }
}

/// Which grid cells are watched: inside an include zone (or anywhere when
/// there is none) and outside every exclude zone.
pub fn mask(zones: &[MotionZone]) -> Vec<bool> {
    let inside = |z: &MotionZone, x: f32, y: f32| x >= z.x && x < z.x + z.w && y >= z.y && y < z.y + z.h;
    let includes: Vec<&MotionZone> = zones.iter().filter(|z| !z.exclude).collect();
    let mut out = Vec::with_capacity(GRID_W * GRID_H);
    for gy in 0..GRID_H {
        for gx in 0..GRID_W {
            let (x, y) = ((gx as f32 + 0.5) / GRID_W as f32, (gy as f32 + 0.5) / GRID_H as f32);
            let included = includes.is_empty() || includes.iter().any(|z| inside(z, x, y));
            out.push(included && !zones.iter().any(|z| z.exclude && inside(z, x, y)));
        }
    }
    out
}

/// Mean brightness of each grid cell (every other pixel and row is enough).
pub fn cells(y: &[u8], width: usize, height: usize, stride: usize) -> Vec<f32> {
    let mut out = vec![0.0; GRID_W * GRID_H];
    if width < GRID_W || height < GRID_H {
        return out;
    }
    for gy in 0..GRID_H {
        let (y0, y1) = (gy * height / GRID_H, (gy + 1) * height / GRID_H);
        for gx in 0..GRID_W {
            let (x0, x1) = (gx * width / GRID_W, (gx + 1) * width / GRID_W);
            let (mut sum, mut n) = (0u32, 0u32);
            for row in (y0..y1).step_by(2) {
                let line = &y[row * stride..row * stride + x1];
                for px in line[x0..x1].iter().step_by(2) {
                    sum += u32::from(*px);
                    n += 1;
                }
            }
            out[gy * GRID_W + gx] = sum as f32 / n.max(1) as f32;
        }
    }
    out
}

/// A change in the detection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Started,
    Ended,
}

pub struct Analyzer {
    thresholds: Thresholds,
    mask: Vec<bool>,
    watched: usize,
    background: Vec<f32>,
    seen: u32,
    hits: u32,
    active: bool,
    /// Stream time (seconds) of the last picture with motion.
    last_motion: f64,
    /// Largest changed share of the watched area since the last `take_peak`.
    peak: f32,
}

impl Analyzer {
    pub fn new(sensitivity: u8, zones: &[MotionZone]) -> Self {
        let mask = mask(zones);
        let watched = mask.iter().filter(|m| **m).count();
        Self { thresholds: Thresholds::from_sensitivity(sensitivity), mask, watched, background: Vec::new(), seen: 0, hits: 0, active: false, last_motion: 0.0, peak: 0.0 }
    }

    #[cfg(test)]
    pub fn active(&self) -> bool {
        self.active
    }

    /// Largest changed area since the last call, and the trigger level
    /// (for diagnostics).
    pub fn take_peak(&mut self) -> (f32, f32) {
        (std::mem::take(&mut self.peak), self.thresholds.min_area)
    }

    /// The stream stopped mid-detection and it was closed from outside:
    /// forget it and learn the scene again.
    pub fn forget(&mut self) {
        self.active = false;
        self.reset();
    }

    /// Start over (new stream parameters or a decoder restart).
    pub fn reset(&mut self) {
        self.background.clear();
        self.seen = 0;
        self.hits = 0;
        // Stream time restarted: a detection in progress ends when quiet.
        self.last_motion = f64::NEG_INFINITY;
    }

    /// Feed one picture's cell brightness taken at stream time `t` seconds.
    pub fn push(&mut self, cells: &[f32], t: f64) -> Option<Transition> {
        let moving = self.moving(cells);
        if moving {
            self.hits += 1;
            self.last_motion = t;
        } else {
            self.hits = 0;
        }
        if !self.active && self.hits >= START_HITS {
            self.active = true;
            return Some(Transition::Started);
        }
        if self.active && !moving && t - self.last_motion >= END_AFTER.as_secs_f64() {
            self.active = false;
            return Some(Transition::Ended);
        }
        None
    }

    /// Whether this picture shows motion; updates the background.
    fn moving(&mut self, cells: &[f32]) -> bool {
        if self.background.len() != cells.len() || self.watched == 0 {
            self.background = cells.to_vec();
            self.seen = 1;
            return false;
        }
        let shift = median_shift(cells, &self.background, &self.mask);
        let changed: Vec<bool> = cells.iter().zip(&self.background).map(|(c, b)| (c - b - shift).abs() > self.thresholds.cell_delta).collect();
        let area = changed.iter().zip(&self.mask).filter(|(c, m)| **c && **m).count() as f32 / self.watched as f32;
        let light_change = area > LIGHT_CHANGE;
        if self.seen > WARMUP {
            self.peak = self.peak.max(area);
        }
        for ((b, c), moved) in self.background.iter_mut().zip(cells).zip(&changed) {
            let rate = if light_change || self.seen < WARMUP { 1.0 } else if *moved { LEARN_MOVING } else { LEARN };
            *b += (c - *b) * rate;
        }
        self.seen = self.seen.saturating_add(1);
        self.seen > WARMUP && !light_change && area >= self.thresholds.min_area
    }
}

/// The typical brightness difference over the watched cells: a moving
/// object changes a part of the picture, a light change moves all of it.
fn median_shift(cells: &[f32], background: &[f32], mask: &[bool]) -> f32 {
    let mut diffs: Vec<f32> = cells.iter().zip(background).zip(mask).filter(|(_, m)| **m).map(|((c, b), _)| c - b).collect();
    if diffs.is_empty() {
        return 0.0;
    }
    let mid = diffs.len() / 2;
    *diffs.select_nth_unstable_by(mid, f32::total_cmp).1
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = GRID_W * GRID_H;

    fn scene(level: f32) -> Vec<f32> {
        vec![level; N]
    }

    /// The scene with a bright block of `w`×`h` cells at (x, y).
    fn with_object(level: f32, x: usize, y: usize, w: usize, h: usize) -> Vec<f32> {
        let mut s = scene(level);
        for gy in y..y + h {
            for gx in x..x + w {
                s[gy * GRID_W + gx] = level + 80.0;
            }
        }
        s
    }

    /// Feed pictures at 5 per second starting at `t`, returning the transitions.
    fn run(a: &mut Analyzer, pictures: &[Vec<f32>], t: &mut f64) -> Vec<Transition> {
        let mut out = Vec::new();
        for p in pictures {
            out.extend(a.push(p, *t));
            *t += 0.2;
        }
        out
    }

    fn learned(sensitivity: u8, zones: &[MotionZone]) -> (Analyzer, f64) {
        let mut a = Analyzer::new(sensitivity, zones);
        let mut t = 0.0;
        assert!(run(&mut a, &vec![scene(100.0); 20], &mut t).is_empty(), "a still scene never triggers");
        (a, t)
    }

    #[test]
    fn an_object_starts_and_ends_a_detection() {
        let (mut a, mut t) = learned(60, &[]);
        assert_eq!(run(&mut a, &vec![with_object(100.0, 4, 4, 3, 3); 3], &mut t), [Transition::Started]);
        // Gone again: ends after the quiet time, not before.
        assert!(run(&mut a, &vec![scene(100.0); 10], &mut t).is_empty(), "2 s quiet: still active");
        assert_eq!(run(&mut a, &vec![scene(100.0); 10], &mut t), [Transition::Ended]);
        assert!(!a.active());
    }

    #[test]
    fn a_single_noisy_picture_is_ignored() {
        let (mut a, mut t) = learned(60, &[]);
        let mut pics = vec![with_object(100.0, 4, 4, 3, 3)];
        pics.extend(vec![scene(100.0); 5]);
        assert!(run(&mut a, &pics, &mut t).is_empty());
    }

    #[test]
    fn a_light_change_is_not_motion() {
        let (mut a, mut t) = learned(100, &[]);
        assert!(run(&mut a, &vec![scene(160.0); 10], &mut t).is_empty(), "the whole picture got brighter");
        // IR switching: every surface changes differently, all over the picture.
        let uneven: Vec<f32> = (0..N).map(|i| 60.0 + 200.0 * (i * 7 % N) as f32 / N as f32).collect();
        assert!(run(&mut a, &vec![uneven; 10], &mut t).is_empty(), "relearned, not motion");
    }

    #[test]
    fn someone_close_to_the_camera_is_motion() {
        // Covers 80 % of the picture: not mistaken for a light change.
        let (mut a, mut t) = learned(60, &[]);
        assert_eq!(run(&mut a, &vec![with_object(100.0, 0, 0, 26, 18); 3], &mut t), [Transition::Started]);
    }

    #[test]
    fn exclusion_zones_mask_motion_and_includes_limit_it() {
        let zone = |x, y, w, h, exclude| MotionZone { id: "z".into(), name: "z".into(), x, y, w, h, exclude };
        // Object in the left half; the left half is excluded.
        let (mut a, mut t) = learned(80, &[zone(0.0, 0.0, 0.5, 1.0, true)]);
        assert!(run(&mut a, &vec![with_object(100.0, 4, 4, 3, 3); 5], &mut t).is_empty());
        // Only the right half is watched: same result.
        let (mut a, mut t) = learned(80, &[zone(0.5, 0.0, 0.5, 1.0, false)]);
        assert!(run(&mut a, &vec![with_object(100.0, 4, 4, 3, 3); 5], &mut t).is_empty());
        // Object on the right: detected.
        assert_eq!(run(&mut a, &vec![with_object(100.0, 24, 4, 3, 3); 3], &mut t), [Transition::Started]);
    }

    #[test]
    fn sensitivity_changes_what_counts() {
        // One cell (0.17 % of the frame) is too small at 60 but enough at 100.
        let (mut low, mut t) = learned(60, &[]);
        assert!(run(&mut low, &vec![with_object(100.0, 4, 4, 1, 1); 5], &mut t).is_empty());
        let (mut high, mut t) = learned(100, &[]);
        assert_eq!(run(&mut high, &vec![with_object(100.0, 4, 4, 2, 1); 3], &mut t), [Transition::Started]);
        assert!(Thresholds::from_sensitivity(100).cell_delta < Thresholds::from_sensitivity(0).cell_delta);
    }

    #[test]
    fn cells_average_the_luma_plane() {
        let (w, h, stride) = (64, 36, 70);
        let mut y = vec![10u8; stride * h];
        for row in 0..2 {
            for px in 0..2 {
                y[row * stride + px] = 250;
            }
        }
        let c = cells(&y, w, h, stride);
        assert_eq!(c.len(), N);
        assert!(c[0] > 200.0, "top-left cell is bright: {}", c[0]);
        assert!((c[1] - 10.0).abs() < 0.01);
    }
}
