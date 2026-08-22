use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::Notify;

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    inner: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
    cancelled: AtomicBool,
    commit_gate: Mutex<()>,
    notify: Notify,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        let _gate = self
            .inner
            .commit_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.inner.cancelled.swap(true, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    pub(crate) async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }

    pub(crate) fn run_if_active<T>(&self, operation: impl FnOnce() -> T) -> Option<T> {
        let _gate = self
            .inner
            .commit_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (!self.is_cancelled()).then(operation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_is_sticky() {
        let token = CancellationToken::new();
        token.cancel();
        token.cancelled().await;
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancelled_token_does_not_enter_commit_gate() {
        let token = CancellationToken::new();
        token.cancel();
        let mut entered = false;
        let result = token.run_if_active(|| {
            entered = true;
        });
        assert!(result.is_none());
        assert!(!entered);
    }
}
