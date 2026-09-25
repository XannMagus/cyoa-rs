use cyoa_application::cancellation::CancellationSource;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn cancellation_is_shared_across_threads_and_isolated_between_requests() {
    let source = CancellationSource::default();
    let token = source.token();
    let other = CancellationSource::default();
    assert!(!token.is_cancelled());
    source.cancel();
    std::thread::spawn(move || assert!(token.is_cancelled()))
        .join()
        .unwrap();
    assert!(source.token().is_cancelled());
    assert!(!other.token().is_cancelled());
}

/// The supervisor registers its wake notifier only once it starts polling,
/// which can race a `cancel()` that already happened while it was still
/// spawning. A notifier registered after `cancel()` has already fired must
/// still run immediately, or the poll loop hangs waiting for a wake signal
/// that already happened (the exact idle-cancel hang this design exists to
/// prevent). This is the hard direction: register-then-cancel is easy and
/// doesn't prove anything about the lost-wakeup case.
#[test]
fn notifier_registered_after_cancel_already_fired_still_runs_immediately() {
    let source = CancellationSource::default();
    let token = source.token();
    source.cancel();

    let ran = Arc::new(AtomicBool::new(false));
    let ran_writer = Arc::clone(&ran);
    token.on_cancel(move || ran_writer.store(true, Ordering::SeqCst));

    assert!(
        ran.load(Ordering::SeqCst),
        "notifier registered after cancel() must run immediately, not be lost"
    );
}

/// The easy direction, for contrast: register before cancel, confirm it still
/// fires exactly once.
#[test]
fn notifier_registered_before_cancel_runs_once_when_cancel_fires() {
    let source = CancellationSource::default();
    let token = source.token();

    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls_writer = Arc::clone(&calls);
    token.on_cancel(move || {
        calls_writer.fetch_add(1, Ordering::SeqCst);
    });

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    source.cancel();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    source.cancel(); // idempotent: must not double-fire
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
