//! Cooperative cancellation shared by use cases and adapters.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// The caller's authority to cancel a generation; workers receive only a token.
#[derive(Debug, Default)]
pub struct CancellationSource(Arc<AtomicBool>);

impl CancellationSource {
    pub fn token(&self) -> CancellationToken {
        CancellationToken(Arc::clone(&self.0))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
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
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
