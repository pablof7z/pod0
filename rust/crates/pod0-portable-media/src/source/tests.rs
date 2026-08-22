use std::{
    error::Error,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use super::{HttpLoadOptions, MediaLoader, MediaSource, RuntimeSource};
use crate::{CancellationToken, MediaError};

#[test]
fn cancellation_does_not_wait_for_a_stalled_blocking_dns_task() {
    let stall = Arc::new(ResolverStall::default());
    let resolver = Arc::new(StalledResolver {
        stall: Arc::clone(&stall),
    });
    let options = HttpLoadOptions {
        connect_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(30),
        ..HttpLoadOptions::default()
    };
    let maximum_response_bytes = options.maximum_response_bytes;
    let client_builder = reqwest::Client::builder()
        .connect_timeout(options.connect_timeout)
        .timeout(options.request_timeout)
        .user_agent(options.user_agent)
        .dns_resolver(resolver);
    let loader =
        MediaLoader::from_client_builder(client_builder, maximum_response_bytes, RuntimeSource::Owned)
            .expect("build HTTP loader");
    let cancellation = CancellationToken::new();
    let load_cancellation = cancellation.clone();
    let (completed_sender, completed_receiver) = std::sync::mpsc::channel();
    let load = thread::spawn(move || {
        let result = loader.load(
            MediaSource::Http(
                url::Url::parse("http://deliberately-stalled.invalid/media.wav").expect("test URL"),
            ),
            &load_cancellation,
        );
        drop(loader);
        completed_sender
            .send(result)
            .expect("report HTTP load completion");
    });

    stall.wait_until_started(Duration::from_secs(2));
    let cancelled_at = Instant::now();
    cancellation.cancel();
    let completion = completed_receiver.recv_timeout(Duration::from_secs(1));
    let cancellation_latency = cancelled_at.elapsed();

    stall.release();
    stall.wait_until_finished(Duration::from_secs(2));
    load.join().expect("HTTP load thread");

    let result = completion.expect("cancellation must include bounded runtime teardown");
    assert!(matches!(result, Err(MediaError::Cancelled)));
    assert!(
        cancellation_latency < Duration::from_secs(1),
        "cancellation took {cancellation_latency:?}"
    );
}

struct StalledResolver {
    stall: Arc<ResolverStall>,
}

impl Resolve for StalledResolver {
    fn resolve(&self, _name: Name) -> Resolving {
        let stall = Arc::clone(&self.stall);
        Box::pin(async move {
            tokio::task::spawn_blocking(move || stall.block_until_released())
                .await
                .map_err(|error| -> Box<dyn Error + Send + Sync> { Box::new(error) })?;
            Ok(Box::new(std::iter::empty()) as Addrs)
        })
    }
}

#[derive(Default)]
struct ResolverStall {
    state: Mutex<ResolverState>,
    changed: Condvar,
}

#[derive(Default)]
struct ResolverState {
    started: bool,
    released: bool,
    finished: bool,
}

impl ResolverStall {
    fn block_until_released(&self) {
        let mut state = self.state.lock().expect("resolver stall lock");
        state.started = true;
        self.changed.notify_all();
        while !state.released {
            state = self.changed.wait(state).expect("resolver stall wait");
        }
        state.finished = true;
        self.changed.notify_all();
    }

    fn release(&self) {
        let mut state = self.state.lock().expect("resolver stall lock");
        state.released = true;
        self.changed.notify_all();
    }

    fn wait_until_started(&self, timeout: Duration) {
        self.wait_for(timeout, |state| state.started, "resolver did not start");
    }

    fn wait_until_finished(&self, timeout: Duration) {
        self.wait_for(timeout, |state| state.finished, "resolver did not finish");
    }

    fn wait_for(
        &self,
        timeout: Duration,
        predicate: impl Fn(&ResolverState) -> bool,
        message: &str,
    ) {
        let state = self.state.lock().expect("resolver stall lock");
        let (state, result) = self
            .changed
            .wait_timeout_while(state, timeout, |state| !predicate(state))
            .expect("resolver stall wait");
        assert!(!result.timed_out() && predicate(&state), "{message}");
    }
}
