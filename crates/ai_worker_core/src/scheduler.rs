//! Task cadence helpers (interval-based; cron deferred).

use std::time::Duration;

use tokio::time::Instant;

use crate::worker_config::TaskSchedule;

pub struct CadenceTicker {
    interval: Duration,
    next: Instant,
}

impl CadenceTicker {
    pub fn from_schedule(schedule: &TaskSchedule) -> Option<Self> {
        match schedule {
            TaskSchedule::OneShot => None,
            TaskSchedule::Cadence { interval_seconds } => {
                let interval = Duration::from_secs(*interval_seconds);
                Some(Self {
                    interval,
                    next: Instant::now() + interval,
                })
            }
        }
    }

    /// Reset after a tick fires.
    pub fn tick_ready(&mut self) -> bool {
        if Instant::now() >= self.next {
            self.next = Instant::now() + self.interval;
            true
        } else {
            false
        }
    }
}
