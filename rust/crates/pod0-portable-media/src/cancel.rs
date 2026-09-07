use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A cheap cloneable cancellation signal for blocking loads and clip exports.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> crate::Result<()> {
        if self.is_cancelled() {
            Err(crate::MediaError::Cancelled)
        } else {
            Ok(())
        }
    }
}
