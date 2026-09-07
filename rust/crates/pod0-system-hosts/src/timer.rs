use std::{
    sync::atomic::{AtomicU8, Ordering},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

use crate::{Capability, HostResult, Operation, SystemHostError};

mod evidence;

use evidence::{cancelled_evidence, fired_evidence, make_plan};

const CAPABILITY: Capability = Capability::WakeTimer;
const WALL_CLOCK_RECHECK_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeDeadline {
    After(Duration),
    At(SystemTime),
}

#[derive(Clone, Copy, Debug)]
pub struct ClockSnapshot {
    pub wall: SystemTime,
    pub monotonic: Instant,
}

#[derive(Clone, Copy, Debug)]
pub struct WakePlan {
    pub requested: WakeDeadline,
    pub scheduled_at: ClockSnapshot,
    pub target_wall: SystemTime,
    pub target_monotonic: Instant,
}

#[derive(Clone, Copy, Debug)]
pub struct WakeFiredEvidence {
    pub plan: WakePlan,
    pub observed_at: ClockSnapshot,
    pub elapsed_monotonic: Duration,
    pub overdue_by: Duration,
}

#[derive(Clone, Copy, Debug)]
pub struct WakeCancelledEvidence {
    pub plan: WakePlan,
    pub observed_at: ClockSnapshot,
    pub elapsed_monotonic: Duration,
}

#[derive(Clone, Copy, Debug)]
pub enum WakeOutcome {
    Fired(WakeFiredEvidence),
    Cancelled(WakeCancelledEvidence),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum TimerState {
    Pending,
    Fired,
    Cancelled,
}

#[derive(Debug)]
struct CancellationState {
    terminal: AtomicU8,
    wait_lock: Mutex<()>,
    changed: Condvar,
}

impl Default for CancellationState {
    fn default() -> Self {
        Self {
            terminal: AtomicU8::new(TimerState::Pending as u8),
            wait_lock: Mutex::new(()),
            changed: Condvar::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WakeCancellation {
    state: Arc<CancellationState>,
}

impl WakeCancellation {
    pub fn cancel(&self) -> bool {
        let _guard = lock_unpoisoned(&self.state.wait_lock);
        let cancelled = self
            .state
            .terminal
            .compare_exchange(
                TimerState::Pending as u8,
                TimerState::Cancelled as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok();
        if cancelled {
            self.state.changed.notify_all();
        }
        cancelled
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        terminal_state(&self.state.terminal) == TimerState::Cancelled
    }
}

pub struct WakeTimer {
    cancellation: WakeCancellation,
    worker: Option<JoinHandle<WakeOutcome>>,
}

impl WakeTimer {
    pub fn schedule(requested: WakeDeadline) -> HostResult<Self> {
        let plan = make_plan(requested)?;
        let cancellation = WakeCancellation {
            state: Arc::new(CancellationState::default()),
        };
        let worker_cancellation = cancellation.clone();
        let worker = thread::Builder::new()
            .name("pod0-wake-timer".to_owned())
            .spawn(move || wait_for_deadline(plan, worker_cancellation))
            .map_err(|error| {
                SystemHostError::from_io(CAPABILITY, Operation::ScheduleWake, error)
            })?;
        Ok(Self {
            cancellation,
            worker: Some(worker),
        })
    }

    #[must_use]
    pub fn cancellation(&self) -> WakeCancellation {
        self.cancellation.clone()
    }

    pub fn cancel(&self) -> bool {
        self.cancellation.cancel()
    }

    pub fn wait(mut self) -> HostResult<WakeOutcome> {
        let worker = self.worker.take().ok_or_else(|| {
            SystemHostError::platform(
                CAPABILITY,
                Operation::WaitForWake,
                "wake timer worker is unavailable",
            )
        })?;
        worker.join().map_err(|_| {
            SystemHostError::platform(
                CAPABILITY,
                Operation::WaitForWake,
                "wake timer worker panicked",
            )
        })
    }
}

impl Drop for WakeTimer {
    fn drop(&mut self) {
        let _ = self.cancellation.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn wait_for_deadline(plan: WakePlan, cancellation: WakeCancellation) -> WakeOutcome {
    let mut guard = lock_unpoisoned(&cancellation.state.wait_lock);
    loop {
        match terminal_state(&cancellation.state.terminal) {
            TimerState::Cancelled => return cancelled_evidence(plan),
            TimerState::Fired => return fired_evidence(plan),
            TimerState::Pending => {
                let Some(remaining) = remaining_until_deadline(plan) else {
                    if cancellation
                        .state
                        .terminal
                        .compare_exchange(
                            TimerState::Pending as u8,
                            TimerState::Fired as u8,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        return fired_evidence(plan);
                    }
                    continue;
                };
                guard = match cancellation.state.changed.wait_timeout(guard, remaining) {
                    Ok((guard, _)) => guard,
                    Err(poisoned) => poisoned.into_inner().0,
                };
            }
        }
    }
}

fn terminal_state(state: &AtomicU8) -> TimerState {
    match state.load(Ordering::Acquire) {
        value if value == TimerState::Pending as u8 => TimerState::Pending,
        value if value == TimerState::Fired as u8 => TimerState::Fired,
        value if value == TimerState::Cancelled as u8 => TimerState::Cancelled,
        _ => unreachable!("wake timer state is private and always valid"),
    }
}

fn remaining_until_deadline(plan: WakePlan) -> Option<Duration> {
    match plan.requested {
        WakeDeadline::After(_) => {
            let now = Instant::now();
            (now < plan.target_monotonic)
                .then(|| plan.target_monotonic.saturating_duration_since(now))
        }
        WakeDeadline::At(target) => target
            .duration_since(SystemTime::now())
            .ok()
            .map(|remaining| remaining.min(WALL_CLOCK_RECHECK_INTERVAL)),
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
