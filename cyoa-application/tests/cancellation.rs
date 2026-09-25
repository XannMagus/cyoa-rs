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

#[test]
fn a_panicking_notifier_does_not_skip_later_notifiers_or_poison_state() {
    let source = CancellationSource::default();
    let token = source.token();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    token.on_cancel(|| panic!("observer failed"));
    let observed = Arc::clone(&calls);
    token.on_cancel(move || {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source.cancel()));
    assert!(
        panic.is_err(),
        "the original notifier panic must still propagate"
    );
    assert!(token.is_cancelled());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    source.cancel();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn dropping_a_subscription_releases_resources_without_waiting_for_cancellation() {
    let source = CancellationSource::default();
    let token = source.token();
    let resource = Arc::new(AtomicBool::new(false));
    let captured = Arc::clone(&resource);
    let registration = token.subscribe(move || captured.store(true, Ordering::SeqCst));
    assert_eq!(Arc::strong_count(&resource), 2);
    drop(registration);
    assert_eq!(Arc::strong_count(&resource), 1);
    source.cancel();
    assert!(!resource.load(Ordering::SeqCst));
}

#[test]
fn scoped_notifications_handle_reuse_late_registration_and_reentrant_callbacks() {
    let source = CancellationSource::default();
    let token = source.token();
    let dropped = token.subscribe(|| panic!("removed callback ran"));
    drop(dropped);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let nested_token = token.clone();
    let registration = token.subscribe(move || {
        let observed = Arc::clone(&observed);
        nested_token.on_cancel(move || {
            observed.fetch_add(1, Ordering::SeqCst);
        });
    });
    source.cancel();
    source.cancel();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(registration);
    let observed = Arc::clone(&calls);
    let late = token.subscribe(move || {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(Arc::strong_count(&calls), 1);
    drop(late);
}
