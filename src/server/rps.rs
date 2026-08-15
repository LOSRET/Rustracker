//! Per-second request rate meter.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::MissedTickBehavior;

#[derive(Clone)]
pub(crate) struct RpsMeter {
    counter: Arc<AtomicU64>,
    current: Arc<AtomicU64>,
}

impl RpsMeter {
    pub(crate) fn new() -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(0)),
            current: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Count one request.
    pub(crate) fn tick(&self) {
        self.counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Requests per second as of the last sample.
    pub(crate) fn current(&self) -> f64 {
        f64::from_bits(self.current.load(Ordering::Relaxed))
    }

    /// Background task: swap the counter into the current RPS every second.
    pub(crate) fn spawn_sampler(&self) {
        let counter = self.counter.clone();
        let current = self.current.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                let count = counter.swap(0, Ordering::Relaxed);
                current.store((count as f64).to_bits(), Ordering::Relaxed);
            }
        });
    }
}
