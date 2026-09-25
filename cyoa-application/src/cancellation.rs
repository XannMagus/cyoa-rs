//! Cooperative cancellation shared by use cases and adapters.

use std::sync::{Arc, Mutex};

/// The flag and its wake notifiers share one lock, so there is no window
/// where `cancel()` can finish flipping the flag without a concurrently
/// registering notifier either being invoked or observing `cancelled` once
/// it does acquire the lock. This closes the lost-wakeup case: a notifier
/// registered after `cancel()` already fired must still run.
#[derive(Default)]
struct Inner {
    cancelled: bool,
    notifiers: Vec<Box<dyn Fn() + Send>>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("cancelled", &self.cancelled)
            .field("notifiers", &self.notifiers.len())
            .finish()
    }
}

/// The caller's authority to cancel a generation; workers receive only a token.
#[derive(Debug, Default)]
pub struct CancellationSource(Arc<Mutex<Inner>>);

impl CancellationSource {
    pub fn token(&self) -> CancellationToken {
        CancellationToken(Arc::clone(&self.0))
    }

    /// Idempotent: a second call is a no-op, and notifiers run at most once.
    pub fn cancel(&self) {
        let drained = {
            let mut inner = self.0.lock().unwrap();
            if inner.cancelled {
                return;
            }
            inner.cancelled = true;
            std::mem::take(&mut inner.notifiers)
        };
        for notify in drained {
            notify();
        }
    }
}

/// A read-only view of cancellation, created by a `CancellationSource`.
///
/// Subprocess adapters must observe this even while stdout is idle, kill and reap
/// the child, and return a cancellation error. The signal alone does not kill
/// anything; blocking on stdout without a cancellation path violates the contract.
/// Dropping the source does not cancel. Create a fresh source for each request.
///
/// ```compile_fail
/// use cyoa_application::cancellation::CancellationSource;
/// let token = CancellationSource::default().token();
/// token.cancel(); // Only the source has cancellation authority.
/// ```
#[derive(Debug, Clone)]
pub struct CancellationToken(Arc<Mutex<Inner>>);

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.lock().unwrap().cancelled
    }

    /// Registers a wake callback invoked at most once, when cancellation
    /// fires. If cancellation has already fired, calls it immediately
    /// (outside the lock) instead of registering — the lost-wakeup case
    /// this method exists to prevent. A supervisor registers exactly one
    /// notifier per request; deregistration isn't needed because each
    /// request gets a fresh `CancellationSource` whose lifetime matches it.
    pub fn on_cancel(&self, notify: impl Fn() + Send + 'static) {
        let mut inner = self.0.lock().unwrap();
        if inner.cancelled {
            drop(inner);
            notify();
        } else {
            inner.notifiers.push(Box::new(notify));
        }
    }
}
