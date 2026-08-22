use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use pod0_facade::{
    HostObservationEnvelope, HostObservationReceipt, LeasedHostObservationEnvelope, Pod0Facade,
    UnixTimestampMilliseconds,
};

use super::Shell;
use crate::host::{HostExecution, HostExecutor};
use crate::protocol::CliError;

const MAX_SYNCHRONOUS_HOST_STEPS: usize = 64;

pub(super) struct HostPump {
    shared: Arc<PumpShared>,
    worker: Option<JoinHandle<()>>,
}

impl HostPump {
    pub(super) fn start(
        facade: Arc<Pod0Facade>,
        host: Arc<HostExecutor>,
    ) -> Result<Self, CliError> {
        let shared = Arc::new(PumpShared {
            facade,
            host,
            execution: Mutex::new(()),
            signal: Mutex::new(PumpSignal::default()),
            wake: Condvar::new(),
        });
        shared.drain_available()?;
        let worker_shared = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("pod0-host-pump".to_owned())
            .spawn(move || worker_shared.run())
            .map_err(|_| {
                CliError::new("host_pump_start", "host pump could not be started", true)
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub(super) fn wake(&self) {
        let mut signal = self
            .shared
            .signal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        signal.generation = signal.generation.wrapping_add(1);
        self.shared.wake.notify_one();
    }

    pub(super) fn drain_available(&self) -> Result<(), CliError> {
        self.wake();
        self.shared.drain_available().map(|_| ())
    }

    pub(super) fn shutdown(&mut self) {
        {
            let mut signal = self
                .shared
                .signal
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            signal.shutdown = true;
            signal.generation = signal.generation.wrapping_add(1);
            self.shared.wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for HostPump {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct PumpShared {
    facade: Arc<Pod0Facade>,
    host: Arc<HostExecutor>,
    execution: Mutex<()>,
    signal: Mutex<PumpSignal>,
    wake: Condvar,
}

impl PumpShared {
    fn run(&self) {
        loop {
            let observed_generation = {
                let signal = self
                    .signal
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if signal.shutdown {
                    return;
                }
                signal.generation
            };
            let processed = self.drain_available().unwrap_or_default();
            let next_at = self.facade.next_host_effect_at().ok().flatten();
            let mut signal = self
                .signal
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if signal.shutdown {
                return;
            }
            if signal.generation != observed_generation {
                continue;
            }
            let timeout = next_at.and_then(wait_duration);
            if timeout == Some(Duration::ZERO) && processed > 0 {
                continue;
            }
            signal = match timeout {
                Some(duration) if duration > Duration::ZERO => {
                    self.wake
                        .wait_timeout(signal, duration)
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0
                }
                _ => self
                    .wake
                    .wait(signal)
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            };
            if signal.shutdown {
                return;
            }
        }
    }

    fn drain_available(&self) -> Result<usize, CliError> {
        let _execution = self
            .execution
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for processed in 0..MAX_SYNCHRONOUS_HOST_STEPS {
            let Some(request) = self
                .facade
                .next_leased_headless_host_requests(1)
                .into_iter()
                .next()
            else {
                return Ok(processed);
            };
            let HostExecution::Observed(observation) = self.host.execute(&request.request) else {
                return Ok(processed);
            };
            let receipt =
                self.facade
                    .record_leased_host_observation(LeasedHostObservationEnvelope {
                        lease: request.lease,
                        observation: HostObservationEnvelope {
                            request_id: request.request.request_id,
                            cancellation_id: request.request.cancellation_id,
                            observed_request_revision: request.request.issued_revision,
                            sequence_number: 1,
                            observed_at: UnixTimestampMilliseconds::new(now_milliseconds()),
                            observation: *observation,
                        },
                    });
            if !matches!(
                receipt,
                HostObservationReceipt::AcceptedTransient { .. }
                    | HostObservationReceipt::Persisted { .. }
            ) {
                return Err(CliError::new(
                    "host_observation_rejected",
                    "the core did not accept the exact host observation",
                    true,
                ));
            }
        }
        Err(CliError::new(
            "host_loop_limit",
            "host work did not quiesce within the bounded loop",
            true,
        ))
    }
}

#[derive(Default)]
struct PumpSignal {
    generation: u64,
    shutdown: bool,
}

impl Shell {
    pub(super) fn run_host_loop(&self) -> Result<(), CliError> {
        self.host_pump
            .as_ref()
            .ok_or_else(|| CliError::new("store_not_open", "open a store first", false))?
            .drain_available()
    }

    pub(super) fn facade(&self) -> Result<&Arc<Pod0Facade>, CliError> {
        self.facade.as_ref().ok_or_else(|| {
            CliError::new(
                "store_not_open",
                "create or open an authoritative store first",
                false,
            )
        })
    }
}

fn wait_duration(value: UnixTimestampMilliseconds) -> Option<Duration> {
    let milliseconds = value.value.saturating_sub(now_milliseconds());
    Some(Duration::from_millis(
        u64::try_from(milliseconds).unwrap_or_default(),
    ))
}

fn now_milliseconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}
