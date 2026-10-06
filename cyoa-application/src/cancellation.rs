//! Cooperative cancellation shared by use cases and adapters.

use std::sync::{Arc, Mutex, Weak};

/// The flag and its wake notifiers share one lock, so there is no window
/// where `cancel()` can finish flipping the flag without a concurrently
/// registering notifier either being invoked or observing `cancelled` once
/// it does acquire the lock. This closes the lost-wakeup case: a notifier
/// registered after `cancel()` already fired must still run.
#[derive(Default)]
struct Inner {
    cancelled: bool,
    notifiers: Vec<Option<Box<dyn Fn() + Send>>>,
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

    /// Idempotent. All callbacks run outside the lock, even if one panics;
    /// the first panic resumes after the remaining callbacks have run.
    pub fn cancel(&self) {
        let drained = {
            let mut inner = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if inner.cancelled {
                return;
            }
            inner.cancelled = true;
            std::mem::take(&mut inner.notifiers)
        };
        let mut first_panic = None;
        for notify in drained.into_iter().flatten() {
            if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(notify)) {
                first_panic.get_or_insert(panic);
            }
        }
        if let Some(panic) = first_panic {
            std::panic::resume_unwind(panic);
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
    /// Scoped wake notification. Dropping the registration releases its
    /// resources and removes the callback unless cancellation already took it.
    /// Callbacks must finish promptly; the token flag remains authoritative.
    pub fn subscribe(&self, notify: impl Fn() + Send + 'static) -> CancellationRegistration {
        CancellationRegistration {
            inner: Arc::downgrade(&self.0),
            slot: self.register(notify),
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cancelled
    }

    /// Registers a wake callback invoked at most once, when cancellation
    /// fires. If cancellation has already fired, calls it immediately
    /// (outside the lock) instead of registering — the lost-wakeup case
    /// this method exists to prevent. This registration persists until
    /// cancellation or source/token destruction; use `subscribe` for resources
    /// whose lifetime is shorter than that of their cancellation source.
    pub fn on_cancel(&self, notify: impl Fn() + Send + 'static) {
        self.register(notify);
    }

    fn register(&self, notify: impl Fn() + Send + 'static) -> Option<usize> {
        let mut inner = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.cancelled {
            drop(inner);
            notify();
            None
        } else {
            let slot = inner
                .notifiers
                .iter()
                .position(Option::is_none)
                .unwrap_or(inner.notifiers.len());
            if slot == inner.notifiers.len() {
                inner.notifiers.push(None);
            }
            inner.notifiers[slot] = Some(Box::new(notify));
            Some(slot)
        }
    }
}

#[must_use = "hold this registration while its wake callback is needed"]
pub struct CancellationRegistration {
    inner: Weak<Mutex<Inner>>,
    slot: Option<usize>,
}

impl Drop for CancellationRegistration {
    fn drop(&mut self) {
        if let (Some(inner), Some(slot)) = (self.inner.upgrade(), self.slot) {
            // Destruction can invoke user-owned Drop code; keep it outside the lock.
            let callback = {
                inner
                    .lock()
                    .unwrap()
                    .notifiers
                    .get_mut(slot)
                    .and_then(Option::take)
            };
            drop(callback);
        }
    }
}
