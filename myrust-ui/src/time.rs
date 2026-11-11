//! Monotonic time across native and wasm builds.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Duration, Instant};

#[cfg(target_arch = "wasm32")]
pub use std::time::Duration;

/// Monotonic timestamp for wasm builds, backed by the host `performance.now`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(u64);

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn myrust_now_ms() -> f64;
}

#[cfg(target_arch = "wasm32")]
impl Instant {
    /// Returns the host monotonic time.
    pub fn now() -> Self {
        let millis = unsafe { myrust_now_ms() };
        Self((millis.max(0.0) * 1_000_000.0) as u64)
    }

    /// Returns elapsed time since `earlier`, saturating at zero.
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }

    /// Returns elapsed time since `earlier`, saturating at zero.
    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.duration_since(earlier)
    }

    /// Returns elapsed time since this timestamp.
    pub fn elapsed(&self) -> Duration {
        Self::now().duration_since(*self)
    }

    /// Adds a duration, returning `None` on overflow.
    pub fn checked_add(&self, duration: Duration) -> Option<Self> {
        self.0
            .checked_add(duration.as_nanos().min(u128::from(u64::MAX)) as u64)
            .map(Self)
    }
}
