//! How long the GPU renderer waits for a drawable (docs/specs/bench.md §6),
//! so the bench can tell the app's own work from its wait for the display.
//!
//! wgpu marks the call that hands out the next drawable with a `profiling`
//! scope, which the `profile-with-tracing` backend makes a tracing span. In
//! bench runs only, a subscriber times that one span and turns every other
//! span off. Outside the bench no subscriber is installed, and tracing's
//! level stays off.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tracing::level_filters::LevelFilter;
use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber};

/// wgpu's scope around handing out the next drawable
/// (`wgpu_core::present::Surface::get_current_texture`).
const ACQUIRE: &str = "Surface::get_current_texture";

/// Every wait so far, in nanoseconds.
static WAITED_NS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// When this thread entered the span.
    static SINCE: Cell<Option<Instant>> = const { Cell::new(None) };
}

struct Acquire;

impl Subscriber for Acquire {
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if self.enabled(metadata) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.is_span() && metadata.name() == ACQUIRE
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::INFO)
    }

    // One kind of span, never nested, so one id does.
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, _: &Event<'_>) {}

    fn enter(&self, _: &Id) {
        SINCE.with(|s| s.set(Some(Instant::now())));
    }

    fn exit(&self, _: &Id) {
        if let Some(t) = SINCE.with(Cell::take) {
            let ns = u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX);
            WAITED_NS.fetch_add(ns, Ordering::Relaxed);
        }
    }
}

/// Starts timing, for a bench run. Does nothing if a subscriber is set.
pub fn install() {
    let _ = tracing::subscriber::set_global_default(Acquire);
}

/// Every wait so far, in milliseconds.
pub fn waited_ms() -> f64 {
    WAITED_NS.load(Ordering::Relaxed) as f64 / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_wait_for_a_drawable_is_timed() {
        install();
        let before = waited_ms();
        {
            let other = tracing::span!(tracing::Level::INFO, "Surface::present");
            let _in = other.enter();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(waited_ms() - before < 1.0);
        {
            let wait = tracing::span!(tracing::Level::INFO, "Surface::get_current_texture");
            let _in = wait.enter();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let waited = waited_ms() - before;
        assert!((19.0..1000.0).contains(&waited), "{waited}");
    }
}
