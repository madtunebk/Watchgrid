//! Simulation helpers: latency, jitter and a deterministic RNG for seeding.

use gloo_timers::future::TimeoutFuture;

/// Simulated network round-trip.
pub async fn latency() {
    TimeoutFuture::new(120 + (js_sys::Math::random() * 180.0) as u32).await;
}

/// `base` ± `spread / 2`, random on every call (for live-looking metrics).
pub fn jitter(base: f64, spread: f64) -> f64 {
    base + (js_sys::Math::random() - 0.5) * spread
}

/// Small deterministic PRNG so seed data is stable between reloads.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407))
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    /// Uniform in `lo..hi` (hi exclusive).
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.next_u32() % (hi - lo).max(1)
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.next_u32() as usize % items.len()]
    }
}
